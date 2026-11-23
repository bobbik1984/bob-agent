//! Local read-only Codex runner. The gated worker is assigned to a kill-on-close
//! Windows Job BEFORE it receives permission to spawn Codex. No shell expansion.
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
struct WorkerInput {
    program: PathBuf,
    directory: PathBuf,
    prompt: String,
}
pub struct Outcome {
    pub stream: String,
    pub exit: Option<i32>,
    pub reason: Option<&'static str>,
    pub cleanup: bool,
}
const LIMIT: usize = 2 * 1024 * 1024;

/// Called before Tauri initialization. EOF or invalid input means no child launch.
pub fn worker_entry() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--bob-readonly-worker") {
        return false;
    }
    let result = (|| -> std::io::Result<i32> {
        let mut data = String::new();
        std::io::stdin()
            .take(512 * 1024)
            .read_to_string(&mut data)?;
        let input: WorkerInput =
            serde_json::from_str(&data).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        if !input.program.is_absolute() || !input.directory.is_absolute() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let mut cmd = Command::new(input.program);
        cmd.args([
            "exec",
            "--ignore-user-config",
            "--ignore-rules",
            "--sandbox",
            "read-only",
            "--ephemeral",
            "--skip-git-repo-check",
            "--json",
            "--color",
            "never",
        ]);
        // These are installed CLI capability names, not model or auth settings.
        for feature in [
            "shell_tool",
            "shell_snapshot",
            "apps",
            "plugins",
            "browser_use",
            "computer_use",
            "image_generation",
            "hooks",
        ] {
            cmd.args(["--disable", feature]);
        }
        cmd.arg("-C")
            .arg(&input.directory)
            .arg("-")
            .current_dir(&input.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn()?;
        let prompt=format!("This Bob session is for read-only text replies in an empty temporary workspace. Do not use tools, read user files, modify files, or run commands. Respond to the following user message as text.\n\n{}",input.prompt);
        if child
            .stdin
            .take()
            .ok_or(std::io::ErrorKind::BrokenPipe)?
            .write_all(prompt.as_bytes())
            .is_err()
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        Ok(child.wait()?.code().unwrap_or(1))
    })();
    std::process::exit(result.unwrap_or(125));
}

pub fn configured_program() -> Option<PathBuf> {
    // Explicit host configuration, never renderer-supplied command or token.
    let p = PathBuf::from(std::env::var_os("BOB_CODEX_EXECUTABLE")?);
    if cfg!(windows) && p.is_absolute() && p.is_file() {
        Some(p)
    } else {
        None
    }
}
fn read_bounded(
    mut stream: impl Read + Send + 'static,
    overflow: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut block = [0u8; 8192];
        loop {
            match stream.read(&mut block) {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = LIMIT.saturating_sub(output.len());
                    output.extend_from_slice(&block[..n.min(remaining)]);
                    if n > remaining {
                        overflow.store(true, Ordering::SeqCst);
                        break;
                    }
                }
                Err(_) => {
                    overflow.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
        output
    })
}
pub fn execute(
    program: &Path,
    directory: &Path,
    prompt: String,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
    job_name: &str,
    ready: impl FnOnce() -> bool,
) -> Outcome {
    #[cfg(windows)]
    {
        execute_windows(program, directory, prompt, cancel, timeout, job_name, ready)
    }
    #[cfg(not(windows))]
    {
        let _ = (program, directory, prompt, cancel, timeout, job_name, ready);
        Outcome {
            stream: String::new(),
            exit: None,
            reason: Some("unsupported_platform"),
            cleanup: true,
        }
    }
}

#[cfg(windows)]
fn execute_windows(
    program: &Path,
    directory: &Path,
    prompt: String,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
    job_name: &str,
    ready: impl FnOnce() -> bool,
) -> Outcome {
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    let fail = |reason| Outcome {
        stream: String::new(),
        exit: None,
        reason: Some(reason),
        cleanup: true,
    };
    if cancel.load(Ordering::SeqCst) {
        return fail("cancelled");
    }
    let job = match job::Job::new(job_name) {
        Ok(j) => j,
        Err(_) => return fail("job_setup_failed"),
    };
    #[cfg(test)]
    let worker_override = std::env::var_os("BOB_TEST_WORKER").map(PathBuf::from);
    #[cfg(not(test))]
    let worker_override: Option<PathBuf> = None;
    let exe = match worker_override
        .map(Ok)
        .unwrap_or_else(std::env::current_exe)
    {
        Ok(p) => p,
        Err(_) => return fail("spawn_failed"),
    };
    let mut child = match Command::new(exe)
        .arg("--bob-readonly-worker")
        .creation_flags(0x08000000)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return fail("spawn_failed"),
    };
    if job.assign(child.as_raw_handle()).is_err() {
        let _ = child.kill();
        let done = child.wait().is_ok();
        return Outcome {
            stream: String::new(),
            exit: None,
            reason: Some("job_assignment_failed"),
            cleanup: done,
        };
    }
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = read_bounded(child.stdout.take().unwrap(), overflow.clone());
    let stderr = read_bounded(child.stderr.take().unwrap(), overflow.clone());
    let input = WorkerInput {
        program: program.into(),
        directory: directory.into(),
        prompt,
    };
    let bytes = serde_json::to_vec(&input).unwrap();
    let mut reason = None;
    if cancel.load(Ordering::SeqCst) {
        reason = Some("cancelled")
    } else if !ready() {
        reason = Some("start_not_confirmed")
    } else if cancel.load(Ordering::SeqCst) {
        reason = Some("cancelled")
    }
    let gate = if reason.is_none() {
        let mut pipe = child.stdin.take().unwrap();
        Some(std::thread::spawn(move || pipe.write_all(&bytes)))
    } else {
        None
    };
    // Always close the gate, including cancel-before-start.
    drop(child.stdin.take());
    let start = Instant::now();
    let mut exit = None;
    loop {
        if reason.is_some() {
            break;
        }
        if cancel.load(Ordering::SeqCst) {
            reason = Some("cancelled");
            break;
        }
        if overflow.load(Ordering::SeqCst) {
            reason = Some("output_limit");
            break;
        }
        if start.elapsed() >= timeout {
            reason = Some("timeout");
            break;
        }
        match child.try_wait() {
            Ok(Some(s)) => {
                exit = s.code();
                break;
            }
            Ok(None) => {}
            Err(_) => {
                reason = Some("io_failed");
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    // A worker exit alone is insufficient: descendants must be gone too.
    let terminated = job.terminate().is_ok();
    let cleanup = terminated && job.wait_empty(Duration::from_secs(5));
    if cleanup {
        let _ = child.wait();
    }
    let stream = if cleanup {
        String::from_utf8(stdout.join().unwrap_or_default()).unwrap_or_default()
    } else {
        String::new()
    };
    if cleanup {
        if let Some(gate) = gate {
            if !matches!(gate.join(), Ok(Ok(()))) && reason.is_none() {
                reason = Some("io_failed")
            }
        }
        let _ = stderr.join();
    } // Credentials/errors are not forwarded to UI logs.
    if overflow.load(Ordering::SeqCst) {
        reason = Some("output_limit")
    }
    if !cleanup {
        reason = Some("cleanup_unconfirmed")
    }
    Outcome {
        stream,
        exit,
        reason,
        cleanup,
    }
}

#[cfg(windows)]
mod job {
    use std::{
        ffi::c_void,
        io,
        time::{Duration, Instant},
    };
    type Handle = *mut c_void;
    #[repr(C)]
    #[derive(Default)]
    struct Basic {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min: usize,
        max: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Limits {
        basic: Basic,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Accounting {
        times: [i64; 4],
        faults: u32,
        total: u32,
        active: u32,
        terminated: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn QueryInformationJobObject(
            job: Handle,
            class: i32,
            info: *mut c_void,
            len: u32,
            returned: *mut u32,
        ) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn OpenJobObjectW(access: u32, inherit: i32, name: *const u16) -> Handle;
        fn GetLastError() -> u32;
    }
    pub struct Job(Handle);
    impl Job {
        pub fn new(name: &str) -> io::Result<Self> {
            unsafe {
                let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                let h = CreateJobObjectW(std::ptr::null(), name.as_ptr());
                if h.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let existed = GetLastError() == 183;
                let job = Self(h);
                if existed {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                let mut limits = Limits::default();
                limits.basic.flags = 0x2000;
                if SetInformationJobObject(
                    h,
                    9,
                    &limits as *const _ as _,
                    std::mem::size_of::<Limits>() as u32,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(job)
            }
        }
        pub fn assign(&self, process: Handle) -> io::Result<()> {
            if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        pub fn terminate(&self) -> io::Result<()> {
            if unsafe { TerminateJobObject(self.0, 1) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        pub fn wait_empty(&self, limit: Duration) -> bool {
            let start = Instant::now();
            loop {
                let mut a = Accounting::default();
                if unsafe {
                    QueryInformationJobObject(
                        self.0,
                        1,
                        &mut a as *mut _ as _,
                        std::mem::size_of::<Accounting>() as u32,
                        std::ptr::null_mut(),
                    )
                } == 0
                {
                    return false;
                }
                if a.active == 0 {
                    return true;
                }
                if start.elapsed() >= limit {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    pub fn absent_or_empty(name: &str) -> bool {
        unsafe {
            let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let h = OpenJobObjectW(4, 0, name.as_ptr());
            if h.is_null() {
                return GetLastError() == 2;
            }
            let job = Job(h);
            job.wait_empty(Duration::ZERO)
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub fn job_name(execution_id: &str) -> String {
    format!("Local\\BobReadOnly-{}", execution_id.replace(':', "_"))
}
pub fn recovered_tree_is_gone(name: &str) -> bool {
    #[cfg(windows)]
    {
        job::absent_or_empty(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        false
    }
}

#[cfg(all(test, windows))]
mod process_tests {
    use super::*;
    fn paths() -> (PathBuf, PathBuf) {
        let program = PathBuf::from(
            std::env::var_os("BOB_TEST_FIXTURE").expect("explicit compiled fixture required"),
        );
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("bob-supervisor-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        (program, dir)
    }
    #[test]
    #[ignore = "requires independently built Bob worker and fake CLI, no model"]
    fn real_process_success_timeout_cancel_and_limit() {
        for (prompt, reason, timeout) in [
            ("fixture-success", None, 20000),
            ("fixture-slow", Some("timeout"), 700),
            ("fixture-cancel", Some("cancelled"), 20000),
            ("fixture-overflow", Some("output_limit"), 20000),
            ("fixture-fail", None, 20000),
        ] {
            let (program, dir) = paths();
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let child_marker = dir.join("fixture-child.pid");
            let name = job_name(&dir.file_name().unwrap().to_string_lossy());
            let result = execute(
                &program,
                &dir,
                prompt.into(),
                cancel,
                Duration::from_millis(timeout),
                &name,
                || {
                    if prompt == "fixture-cancel" {
                        std::thread::spawn(move || {
                            let wait = Instant::now();
                            while !child_marker.exists() && wait.elapsed() < Duration::from_secs(5)
                            {
                                std::thread::sleep(Duration::from_millis(30));
                            }
                            flag.store(true, Ordering::SeqCst);
                        });
                    }
                    true
                },
            );
            assert_eq!(result.reason, reason, "{prompt}");
            assert!(result.cleanup, "{prompt}");
            assert!(recovered_tree_is_gone(&name));
            if prompt == "fixture-success" {
                assert_eq!(result.exit, Some(0));
                assert!(result.stream.contains("FIXTURE_REPLY"));
            }
            if prompt == "fixture-fail" {
                assert_eq!(result.exit, Some(7));
            }
        }
    }
    #[test]
    #[ignore = "requires independently built Bob worker and fake CLI, no model"]
    fn kill_owner_terminates_descendants_without_model_replay() {
        let (_, dir) = paths();
        let ready = dir.join("ready");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runner::process_tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("BOB_TEST_READY", &ready)
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(30));
        }
        let data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ready).unwrap()).unwrap();
        let name = data["job"].as_str().unwrap();
        let marker = PathBuf::from(data["directory"].as_str().unwrap()).join("fixture-child.pid");
        while !marker.exists() {
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(30));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let start = Instant::now();
        while !recovered_tree_is_gone(&name) {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    #[test]
    #[ignore = "subprocess only"]
    fn crash_child() {
        let (program, dir) = paths();
        let name = job_name(&dir.file_name().unwrap().to_string_lossy());
        let _ = execute(
            &program,
            &dir,
            "fixture-slow".into(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(60),
            &name,
            || {
                std::fs::write(
                    std::env::var_os("BOB_TEST_READY").unwrap(),
                    serde_json::json!({"job":name,"directory":dir}).to_string(),
                )
                .unwrap();
                true
            },
        );
    }
}

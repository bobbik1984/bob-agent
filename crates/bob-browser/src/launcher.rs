use crate::error::{BrowserError, BrowserResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STALE_LOCK_ARTIFACTS: &[&str] = &[
    "SingletonLock",
    "SingletonCookie",
    "SingletonSocket",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub binary_path: Option<PathBuf>,
    pub user_data_dir: PathBuf,
    pub remote_debugging_port: u16, // 0 for ephemeral
    pub window_width: u32,
    pub window_height: u32,
    pub idle_tab_timeout_secs: u64,
    pub headless: bool,
    pub additional_args: Vec<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            user_data_dir: std::env::temp_dir().join("bob_browser_profile"),
            remote_debugging_port: 0,
            window_width: 390,
            window_height: 844,
            idle_tab_timeout_secs: 600, // 10 minutes
            headless: true,
            additional_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabState {
    pub target_id: String,
    pub url: String,
    pub ws_url: String,
    pub last_active_at: u64,
    pub is_sleeping: bool,
}

pub struct ChromiumManager {
    config: BrowserConfig,
    child_process: Option<Child>,
    active_port: Option<u16>,
    tabs: HashMap<String, TabState>,
    pub restart_count: u32,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Discovers installed Chromium / Chrome / Edge binary cross-platform.
/// Checks `BOB_CHROMIUM_BIN` and `CHROME_BIN` environment variables first.
pub fn find_chromium_binary() -> BrowserResult<PathBuf> {
    for env_key in ["BOB_CHROMIUM_BIN", "CHROME_BIN"] {
        if let Ok(env_bin) = std::env::var(env_key) {
            let trimmed = env_bin.trim();
            if !trimmed.is_empty() {
                let p = PathBuf::from(trimmed);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
    }

    let candidate_paths: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
    } else {
        &[
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/microsoft-edge",
            "/usr/bin/microsoft-edge-stable",
            "/snap/bin/chromium",
        ]
    };

    for path_str in candidate_paths {
        let p = PathBuf::from(path_str);
        if p.is_file() {
            return Ok(p);
        }
    }

    if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let p1 = PathBuf::from(&local_app_data).join(r"Google\Chrome\Application\chrome.exe");
            if p1.is_file() {
                return Ok(p1);
            }
            let p2 = PathBuf::from(&local_app_data).join(r"Microsoft\Edge\Application\msedge.exe");
            if p2.is_file() {
                return Ok(p2);
            }
        }
    }

    Err(BrowserError::BinaryNotFound)
}

/// Ensures `user_data_dir` exists with strict `0700` permissions on Unix, verifies writability,
/// and optionally cleans stale Chromium singleton lock files/dangling symlinks.
pub fn secure_and_prepare_profile_dir(
    user_data_dir: &Path,
    clean_stale_locks: bool,
) -> Result<(), String> {
    fs::create_dir_all(user_data_dir)
        .map_err(|e| format!("failed to create user_data_dir {}: {}", user_data_dir.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(user_data_dir, fs::Permissions::from_mode(0o700)).map_err(|e| {
            format!(
                "failed to chmod 0700 {}: {}",
                user_data_dir.display(),
                e
            )
        })?;
        let meta = fs::metadata(user_data_dir).map_err(|e| {
            format!(
                "failed to stat {}: {}",
                user_data_dir.display(),
                e
            )
        })?;
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o700 {
            return Err(format!(
                "insecure permissions {:04o} on {} (expected 0700)",
                mode,
                user_data_dir.display()
            ));
        }
    }

    // Verify writability with a short-lived probe file
    let probe_path = user_data_dir.join(format!(".bob_write_probe_{}", std::process::id()));
    {
        let mut f = fs::File::create(&probe_path).map_err(|e| {
            format!(
                "user_data_dir {} is not writable: {}",
                user_data_dir.display(),
                e
            )
        })?;
        f.write_all(b"ok")
            .map_err(|e| format!("failed writing probe to {}: {}", probe_path.display(), e))?;
    }
    let _ = fs::remove_file(&probe_path);

    if clean_stale_locks {
        for lock_name in STALE_LOCK_ARTIFACTS {
            let lock_path = user_data_dir.join(lock_name);
            // symlink_metadata succeeds even when SingletonLock is a dangling symlink!
            if fs::symlink_metadata(&lock_path).is_ok() {
                fs::remove_file(&lock_path).map_err(|e| {
                    format!(
                        "failed to remove stale Chromium lock {}: {}",
                        lock_path.display(),
                        e
                    )
                })?;
            }
        }
    }

    Ok(())
}

fn should_disable_sandbox() -> bool {
    if let Ok(val) = std::env::var("BOB_BROWSER_NO_SANDBOX") {
        let trimmed = val.trim();
        if trimmed == "1" || trimmed.eq_ignore_ascii_case("true") {
            return true;
        }
    }
    Path::new("/.dockerenv").exists() || Path::new("/run/.containerenv").exists()
}

fn build_local_http_client(timeout: Duration) -> BrowserResult<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()?)
}

fn probe_cdp_version(port: u16, timeout: Duration) -> BrowserResult<String> {
    let client = build_local_http_client(timeout)?;
    let url = format!("http://127.0.0.1:{}/json/version", port);
    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        return Err(BrowserError::LaunchFailed(format!(
            "CDP /json/version returned HTTP {}",
            resp.status()
        )));
    }
    let body: serde_json::Value = resp.json()?;
    let ws_url = body
        .get("webSocketDebuggerUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            BrowserError::LaunchFailed(
                "CDP /json/version missing webSocketDebuggerUrl".to_string(),
            )
        })?;
    Ok(ws_url.to_string())
}

fn is_internal_target_url(url: &str) -> bool {
    url.starts_with("chrome-extension://")
        || url.starts_with("devtools://")
        || url.starts_with("edge://")
        || url.starts_with("chrome://")
}

impl ChromiumManager {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            child_process: None,
            active_port: None,
            tabs: HashMap::new(),
            restart_count: 0,
        }
    }

    pub fn config(&self) -> &BrowserConfig {
        &self.config
    }

    pub fn active_port(&self) -> Option<u16> {
        self.active_port
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child_process.as_ref().map(|c| c.id())
    }

    pub fn active_tabs(&self) -> &HashMap<String, TabState> {
        &self.tabs
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child_process {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// Ensures Chromium is running and its CDP `/json/version` endpoint is responsive.
    /// If a previously started Chromium instance crashed, exited, or stopped answering CDP,
    /// cleans up residual processes and stale singleton locks, increments `restart_count`,
    /// and relaunches on the same `user_data_dir` (preserving cookies and storage).
    pub fn ensure_healthy(&mut self) -> BrowserResult<u16> {
        let had_previous = self.child_process.is_some() || self.active_port.is_some();

        if self.is_running() {
            if let Some(port) = self.active_port {
                if probe_cdp_version(port, Duration::from_millis(1200)).is_ok() {
                    return Ok(port);
                }
            }
        }

        if had_previous {
            self.kill_residual_process();
            self.tabs.clear();
            self.restart_count = self.restart_count.saturating_add(1);
        }

        self.ensure_running()
    }

    /// Launches headless Chromium if not already running and waits for CDP `/json/version` readiness.
    pub fn ensure_running(&mut self) -> BrowserResult<u16> {
        if self.is_running() {
            if let Some(port) = self.active_port {
                if probe_cdp_version(port, Duration::from_millis(1200)).is_ok() {
                    return Ok(port);
                }
            }
        }
        self.spawn_process()
    }

    pub fn spawn_process(&mut self) -> BrowserResult<u16> {
        let bin_path = if let Some(ref p) = self.config.binary_path {
            p.clone()
        } else {
            find_chromium_binary()?
        };

        secure_and_prepare_profile_dir(&self.config.user_data_dir, true)
            .map_err(BrowserError::LaunchFailed)?;

        let active_port_file = self.config.user_data_dir.join("DevToolsActivePort");
        if fs::symlink_metadata(&active_port_file).is_ok() {
            let _ = fs::remove_file(&active_port_file);
        }

        let mut cmd = Command::new(&bin_path);
        if self.config.headless {
            cmd.arg("--headless=new");
        }

        cmd.arg(format!(
            "--user-data-dir={}",
            self.config.user_data_dir.display()
        ))
        .arg(format!(
            "--remote-debugging-port={}",
            self.config.remote_debugging_port
        ))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--disable-dev-shm-usage")
        .arg("--font-render-hinting=none")
        .arg("--lang=zh-CN")
        .arg("--disable-gpu")
        .arg("--mute-audio")
        .arg("--disable-extensions")
        .arg("--password-store=basic")
        .arg("--enable-aggressive-domstorage-flushing")
        .arg(format!(
            "--window-size={},{}",
            self.config.window_width, self.config.window_height
        ));

        if should_disable_sandbox()
            && !self
                .config
                .additional_args
                .iter()
                .any(|a| a == "--no-sandbox")
        {
            cmd.arg("--no-sandbox");
        }

        for arg in &self.config.additional_args {
            cmd.arg(arg);
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| BrowserError::LaunchFailed(format!("{}: {}", bin_path.display(), e)))?;

        self.child_process = Some(child);

        match self.wait_until_cdp_ready(&active_port_file) {
            Ok(port) => {
                self.active_port = Some(port);
                Ok(port)
            }
            Err(err) => {
                self.kill_residual_process();
                Err(err)
            }
        }
    }

    fn wait_until_cdp_ready(&mut self, active_port_file: &Path) -> BrowserResult<u16> {
        let start = Instant::now();
        let timeout = Duration::from_secs(6);
        let mut candidate_port = if self.config.remote_debugging_port > 0 {
            Some(self.config.remote_debugging_port)
        } else {
            None
        };

        while start.elapsed() < timeout {
            if let Some(ref mut child) = self.child_process {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(BrowserError::LaunchFailed(format!(
                        "Chromium exited prematurely during startup with status {}",
                        status
                    )));
                }
            }

            if candidate_port.is_none() && active_port_file.exists() {
                if let Ok(content) = fs::read_to_string(active_port_file) {
                    if let Some(first_line) = content.lines().next() {
                        if let Ok(p) = first_line.trim().parse::<u16>() {
                            if p > 0 {
                                candidate_port = Some(p);
                            }
                        }
                    }
                }
            }

            if let Some(port) = candidate_port {
                if probe_cdp_version(port, Duration::from_millis(500)).is_ok() {
                    return Ok(port);
                }
            }

            std::thread::sleep(Duration::from_millis(80));
        }

        Err(BrowserError::LaunchFailed(
            "Timed out waiting for Chromium CDP /json/version readiness".to_string(),
        ))
    }

    /// Opens a new tab for `url` and returns `(target_id, webSocketDebuggerUrl)`.
    pub fn open_page_ws_url(&mut self, url: &str) -> Result<(String, String), String> {
        let port = self.ensure_healthy().map_err(|e| e.to_string())?;
        let client = build_local_http_client(Duration::from_secs(5)).map_err(|e| e.to_string())?;

        let new_endpoint = format!("http://127.0.0.1:{}/json/new?{}", port, url);
        let resp = match client.put(&new_endpoint).send() {
            Ok(r) if r.status().is_success() => r,
            _ => client
                .get(&new_endpoint)
                .send()
                .map_err(|e| format!("CDP /json/new request failed: {}", e))?,
        };

        if !resp.status().is_success() {
            return Err(format!("CDP /json/new returned HTTP {}", resp.status()));
        }

        let target: serde_json::Value = resp
            .json()
            .map_err(|e| format!("invalid JSON from CDP /json/new: {}", e))?;

        let id = target
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing id in /json/new response".to_string())?
            .to_string();
        let ws = target
            .get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing webSocketDebuggerUrl in /json/new response".to_string())?
            .to_string();

        self.tabs.insert(
            id.clone(),
            TabState {
                target_id: id.clone(),
                url: url.to_string(),
                ws_url: ws.clone(),
                last_active_at: now_unix_secs(),
                is_sleeping: false,
            },
        );

        Ok((id, ws))
    }

    pub fn get_or_create_page(&mut self) -> BrowserResult<(String, String)> {
        let port = self.ensure_healthy()?;
        let client = build_local_http_client(Duration::from_secs(3))?;

        let list_url = format!("http://127.0.0.1:{}/json/list", port);
        if let Ok(resp) = client.get(&list_url).send() {
            if let Ok(targets) = resp.json::<Vec<serde_json::Value>>() {
                for target in targets {
                    let t_type = target.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let url = target.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if t_type == "page" && !is_internal_target_url(url) {
                        if let (Some(id), Some(ws)) = (
                            target.get("id").and_then(|v| v.as_str()),
                            target.get("webSocketDebuggerUrl").and_then(|v| v.as_str()),
                        ) {
                            self.tabs.insert(
                                id.to_string(),
                                TabState {
                                    target_id: id.to_string(),
                                    url: url.to_string(),
                                    ws_url: ws.to_string(),
                                    last_active_at: now_unix_secs(),
                                    is_sleeping: false,
                                },
                            );
                            return Ok((id.to_string(), ws.to_string()));
                        }
                    }
                }
            }
        }

        self.open_page_ws_url("about:blank")
            .map_err(BrowserError::Other)
    }

    pub fn record_tab_active(&mut self, target_id: &str) {
        if let Some(tab) = self.tabs.get_mut(target_id) {
            tab.last_active_at = now_unix_secs();
            tab.is_sleeping = false;
        }
    }

    pub fn check_idle_tabs(&mut self, now: u64) -> BrowserResult<Vec<String>> {
        let mut hibernated = Vec::new();
        let timeout = self.config.idle_tab_timeout_secs;
        let port = match self.active_port {
            Some(p) => p,
            None => return Ok(hibernated),
        };

        let client = build_local_http_client(Duration::from_secs(2))?;

        for (id, tab) in self.tabs.iter_mut() {
            if !tab.is_sleeping && now.saturating_sub(tab.last_active_at) > timeout {
                let close_url = format!("http://127.0.0.1:{}/json/close/{}", port, id);
                let _ = client.get(&close_url).send();
                tab.is_sleeping = true;
                hibernated.push(id.clone());
            }
        }

        Ok(hibernated)
    }

    /// Simulates an abrupt browser crash by killing the process group without clearing manager state,
    /// allowing `ensure_healthy()` to exercise crash detection and auto-restart.
    pub fn simulate_crash(&mut self) {
        if let Some(ref mut child) = self.child_process {
            let pid = child.id();
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-KILL", &format!("-{}", pid)])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn kill_residual_process(&mut self) {
        if let Some(mut child) = self.child_process.take() {
            let pid = child.id();
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-KILL", &format!("-{}", pid)])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        self.active_port = None;
    }

    pub fn stop(&mut self) {
        self.kill_residual_process();
        self.tabs.clear();
    }
}

impl Drop for ChromiumManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod launcher_verification_tests {
    use super::*;
    use crate::cdp::CdpClient;
    use crate::screencast::{ScreencastSession, TakeoverCommand, REDACTED_TAKEOVER_TEXT};
    use base64::Engine;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use tempfile::tempdir;

    #[test]
    fn test_secure_and_prepare_profile_dir_enforces_0700_and_removes_dangling_locks() {
        let td = tempdir().expect("tempdir");
        let profile_dir = td.path().join("browser_profile");
        fs::create_dir_all(&profile_dir).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            // Start with overly permissive 0755 permissions and dangling Chromium lock symlinks
            fs::set_permissions(&profile_dir, fs::Permissions::from_mode(0o755)).unwrap();
            symlink("stale-container-host-99999", profile_dir.join("SingletonLock")).unwrap();
            symlink("987654321", profile_dir.join("SingletonCookie")).unwrap();
            symlink(
                "/nonexistent/SingletonSocket",
                profile_dir.join("SingletonSocket"),
            )
            .unwrap();
        }

        assert!(fs::symlink_metadata(profile_dir.join("SingletonLock")).is_ok());

        secure_and_prepare_profile_dir(&profile_dir, true).expect("prepare profile dir");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&profile_dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "profile_dir must have mode 0700");
        }

        for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            assert!(
                fs::symlink_metadata(profile_dir.join(name)).is_err(),
                "stale lock artifact {} should have been removed",
                name
            );
        }
    }

    #[test]
    fn test_real_headless_chromium_cjk_slider_ime_and_crash_cookie_recovery() {
        if Path::new("/usr/bin/microsoft-edge").is_file()
            || Path::new("/usr/bin/chromium").is_file()
        {
            assert!(
                find_chromium_binary().is_ok(),
                "Chromium/Edge binary is installed on host and must be discovered"
            );
        }

        let Ok(bin_path) = find_chromium_binary() else {
            eprintln!("Skipping real headless Chromium test: no browser binary installed");
            return;
        };

        // 1. Start local ephemeral HTTP test server (127.0.0.1:0)
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral test HTTP server");
        listener.set_nonblocking(true).unwrap();
        let server_addr = listener.local_addr().unwrap();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);
        let received_requests: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let reqs_clone = Arc::clone(&received_requests);

        let html_page = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=390, initial-scale=1.0" />
  <title>Bob Headless CJK Verification</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { width: 390px; padding: 16px; background: #ffffff; color: #111827; font-family: "Noto Sans CJK SC", "Noto Sans CJK", sans-serif; }
    p { font-size: 18px; line-height: 1.5; margin-bottom: 12px; }
    #slider-track {
      position: relative;
      width: 320px;
      height: 48px;
      margin-top: 12px;
      background: #e5e7eb;
      border-radius: 8px;
      user-select: none;
    }
    #slider-thumb {
      position: absolute;
      left: 0px;
      top: 0px;
      width: 48px;
      height: 48px;
      background: #2563eb;
      border-radius: 8px;
    }
    #cjk-input {
      display: block;
      margin-top: 16px;
      width: 320px;
      height: 40px;
      font-size: 16px;
      padding: 6px;
    }
  </style>
</head>
<body>
  <p id="cjk-p1">中文渲染测试 · 日本語テスト · 한국어</p>
  <p id="cjk-p2">移动视口滑块验证与输入法组合提交测试。</p>
  <div id="slider-track"><div id="slider-thumb"></div></div>
  <input id="cjk-input" type="text" />
  <script>
    window.__sliderUnlocked = false;
    window.__buttonsSeen = [];
    (function() {
      const thumb = document.getElementById('slider-thumb');
      let dragging = false;
      let startX = 0;
      const onDown = (e) => { dragging = true; startX = e.clientX; };
      const onMove = (e) => {
        if (!dragging) return;
        window.__buttonsSeen.push(e.buttons);
        if (e.buttons !== 1) return;
        const dx = Math.max(0, Math.min(260, e.clientX - startX));
        thumb.style.left = dx + 'px';
        if (dx >= 210) {
          window.__sliderUnlocked = true;
        }
      };
      const onUp = () => { dragging = false; };
      thumb.addEventListener('mousedown', onDown);
      thumb.addEventListener('pointerdown', onDown);
      window.addEventListener('mousemove', onMove);
      window.addEventListener('pointermove', onMove);
      window.addEventListener('mouseup', onUp);
      window.addEventListener('pointerup', onUp);
    })();
  </script>
</body>
</html>"#;

        let server_thread = std::thread::spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        if n == 0 {
                            continue;
                        }
                        let req_str = String::from_utf8_lossy(&buf[..n]);
                        let path = req_str
                            .lines()
                            .next()
                            .and_then(|l| l.split_whitespace().nth(1))
                            .unwrap_or("/")
                            .to_string();
                        let mut cookie_hdr = String::new();
                        for line in req_str.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("cookie") {
                                    cookie_hdr = v.trim().to_string();
                                }
                            }
                        }
                        reqs_clone.lock().unwrap().push((path.clone(), cookie_hdr));

                        if path.starts_with("/favicon") {
                            let _ = stream.write_all(
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            );
                            continue;
                        }

                        // Only set cookie on /login; /recheck sends NO Set-Cookie so persisted cookies are proven!
                        let set_cookie_header = if path.starts_with("/login") {
                            "Set-Cookie: bob_session=verified_persist_123; Max-Age=3600; Path=/\r\n"
                        } else {
                            ""
                        };
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                            set_cookie_header,
                            html_page.len(),
                            html_page
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(15));
                    }
                    Err(_) => break,
                }
            }
        });

        // 2. Prepare temporary user_data_dir with pre-seeded stale SingletonLock symlink
        let td = tempdir().expect("tempdir for profile");
        let profile_dir = td.path().join("browser_profile");
        fs::create_dir_all(&profile_dir).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                "stale-container-instance-4242",
                profile_dir.join("SingletonLock"),
            )
            .unwrap();
        }

        let mut manager = ChromiumManager::new(BrowserConfig {
            binary_path: Some(bin_path),
            user_data_dir: profile_dir.clone(),
            remote_debugging_port: 0,
            window_width: 390,
            window_height: 844,
            headless: true,
            ..Default::default()
        });

        let cdp_port = manager.ensure_healthy().expect("launch headless Chromium");
        assert!(cdp_port > 0);
        assert_eq!(manager.restart_count, 0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&profile_dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }

        // 3. Open test page and drive via CdpClient + ScreencastSession
        let login_url = format!("http://{}/login", server_addr);
        let (_target_id, ws_url) = manager
            .open_page_ws_url(&login_url)
            .expect("open login page");

        let cdp = CdpClient::connect(&ws_url).expect("connect CDP client");
        cdp.enable_default_domains().expect("enable CDP domains");

        // Wait until DOM #slider-thumb is rendered
        let wait_start = Instant::now();
        while wait_start.elapsed() < Duration::from_secs(5) {
            if let Ok(ready) = cdp.evaluate("document.readyState === 'complete' && !!document.getElementById('slider-thumb')") {
                if ready == json!(true) {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let mut session = ScreencastSession::new();
        session.start(&cdp, 390, 844).expect("start 390x844 screencast");

        // Verify CJK glyph rendering metrics
        let cjk_metrics = cdp
            .evaluate(
                r#"(() => {
                    const p = document.getElementById('cjk-p1');
                    const rect = p.getBoundingClientRect();
                    const canvas = document.createElement('canvas');
                    const ctx = canvas.getContext('2d');
                    ctx.font = '18px sans-serif';
                    const m = ctx.measureText('中文渲染测试 · 日本語テスト · 한국어');
                    return { text: p.textContent, rectWidth: rect.width, rectHeight: rect.height, glyphWidth: m.width };
                })()"#,
            )
            .expect("evaluate CJK metrics");
        assert_eq!(
            cjk_metrics["text"],
            "中文渲染测试 · 日本語テスト · 한국어"
        );
        assert!(
            cjk_metrics["rectWidth"].as_f64().unwrap_or(0.0) > 100.0,
            "CJK paragraph must have valid layout width: {cjk_metrics}"
        );
        assert!(
            cjk_metrics["glyphWidth"].as_f64().unwrap_or(0.0) > 150.0,
            "CJK glyphs must measure non-zero width: {cjk_metrics}"
        );

        // Capture a real JPEG screencast frame
        let _ = cdp.evaluate("document.body.style.backgroundColor = '#fefefe'");
        let mut got_jpeg_frame = false;
        let frame_wait = Instant::now();
        while frame_wait.elapsed() < Duration::from_secs(4) {
            let _ = cdp.evaluate("window.scrollBy(0, 0)");
            for evt in cdp.drain_events() {
                if let Some((frame_msg, ack_id)) = session.process_screencast_event(&evt) {
                    let (ack_m, ack_p) = ScreencastSession::build_screencast_ack(ack_id);
                    let _ = cdp.send_command(ack_m, Some(ack_p), Duration::from_secs(1));
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(&frame_msg.data_base64)
                        .expect("valid base64 JPEG frame");
                    if bytes.len() > 200 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
                        got_jpeg_frame = true;
                        break;
                    }
                }
            }
            if got_jpeg_frame {
                break;
            }
            std::thread::sleep(Duration::from_millis(60));
        }
        assert!(got_jpeg_frame, "Expected non-empty JPEG screencast frame from headless Chromium");

        // 4. Test smooth 390px mobile slider drag (requires buttons === 1 on mouseMoved)
        let coords = cdp
            .evaluate(
                r#"(() => {
                    const t = document.getElementById('slider-thumb').getBoundingClientRect();
                    const i = document.getElementById('cjk-input').getBoundingClientRect();
                    return {
                        tx: t.left + t.width / 2,
                        ty: t.top + t.height / 2,
                        ix: i.left + i.width / 2,
                        iy: i.top + i.height / 2
                    };
                })()"#,
            )
            .expect("get element coordinates");
        let tx = coords["tx"].as_f64().unwrap();
        let ty = coords["ty"].as_f64().unwrap();
        let ix = coords["ix"].as_f64().unwrap();
        let iy = coords["iy"].as_f64().unwrap();

        session
            .dispatch(
                &cdp,
                &TakeoverCommand::PointerDown {
                    x: tx,
                    y: ty,
                    button: 0,
                },
            )
            .unwrap();
        for step in 1..=10 {
            let cur_x = tx + (240.0 * (step as f64) / 10.0);
            session
                .dispatch(&cdp, &TakeoverCommand::PointerMove { x: cur_x, y: ty })
                .unwrap();
        }
        session
            .dispatch(
                &cdp,
                &TakeoverCommand::PointerUp {
                    x: tx + 240.0,
                    y: ty,
                    button: 0,
                },
            )
            .unwrap();

        let unlocked = cdp.evaluate("window.__sliderUnlocked").expect("check slider");
        assert_eq!(
            unlocked,
            json!(true),
            "Slider drag with buttons:1 must unlock window.__sliderUnlocked"
        );

        // 5. Focus #cjk-input and test IME composition + commit + masked redaction
        session
            .dispatch(
                &cdp,
                &TakeoverCommand::PointerDown {
                    x: ix,
                    y: iy,
                    button: 0,
                },
            )
            .unwrap();
        session
            .dispatch(
                &cdp,
                &TakeoverCommand::PointerUp {
                    x: ix,
                    y: iy,
                    button: 0,
                },
            )
            .unwrap();

        session
            .dispatch(
                &cdp,
                &TakeoverCommand::ImeSetComposition {
                    text: "nihaoshijie".to_string(),
                    selection_start: Some(0),
                    selection_end: Some(11),
                    masked: false,
                },
            )
            .unwrap();
        let composing_val = cdp
            .evaluate("document.getElementById('cjk-input').value")
            .unwrap();
        assert_eq!(composing_val, json!("nihaoshijie"));

        session
            .dispatch(
                &cdp,
                &TakeoverCommand::ImeCommit {
                    text: "你好世界".to_string(),
                    masked: false,
                },
            )
            .unwrap();
        let committed_val = cdp
            .evaluate("document.getElementById('cjk-input').value")
            .unwrap();
        assert_eq!(
            committed_val,
            json!("你好世界"),
            "ImeCommit must replace active composition with final CJK text"
        );

        // Masked command must be redacted in recorded_commands
        let _ = session.record_command(&TakeoverCommand::ImeCommit {
            text: "SensitiveOtp654321".to_string(),
            masked: true,
        });
        let rec_str = serde_json::to_string(&session.recorded_commands).unwrap();
        assert!(!rec_str.contains("SensitiveOtp654321"));
        assert!(rec_str.contains(REDACTED_TAKEOVER_TEXT));

        // Complete takeover (also flushes cookies to SQLite disk!)
        session.dispatch(&cdp, &TakeoverCommand::Complete).unwrap();
        drop(cdp);

        // Allow background SQLite task runner a brief moment to finish writing Cookies file
        std::thread::sleep(Duration::from_millis(250));

        // 6. Force-kill Chromium process and verify auto-restart + cookie persistence!
        manager.simulate_crash();
        assert!(!manager.is_running());

        let restarted_port = manager
            .ensure_healthy()
            .expect("auto-restart Chromium after crash");
        assert!(restarted_port > 0);
        assert_eq!(
            manager.restart_count, 1,
            "ensure_healthy must increment restart_count to 1 after crash"
        );

        let recheck_url = format!("http://{}/recheck", server_addr);
        let (_new_tid, new_ws) = manager
            .open_page_ws_url(&recheck_url)
            .expect("open recheck page after restart");
        let cdp_after = CdpClient::connect(&new_ws).expect("connect CDP after restart");
        let wait_recheck = Instant::now();
        let mut saw_persisted_cookie = false;
        while wait_recheck.elapsed() < Duration::from_secs(5) {
            {
                let reqs = received_requests.lock().unwrap();
                if reqs.iter().any(|(p, c)| {
                    p.starts_with("/recheck") && c.contains("bob_session=verified_persist_123")
                }) {
                    saw_persisted_cookie = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(60));
        }
        drop(cdp_after);
        manager.stop();
        stop_flag.store(true, Ordering::Relaxed);
        let _ = server_thread.join();

        assert!(
            saw_persisted_cookie,
            "Reloading after Chromium crash + auto-restart must send persisted Cookie: bob_session=verified_persist_123"
        );
    }
}

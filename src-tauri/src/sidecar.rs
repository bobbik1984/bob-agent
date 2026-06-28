use std::sync::Mutex;
use tauri::{AppHandle, State, command, Manager};
use std::process::{Command, Child};
use serde_json::{json, Value};

pub struct SidecarState {
    pub child: Mutex<Option<Child>>,
}

// ═══════════════════════════════════════════════════════════
// Windows Job Object — 确保子进程跟随主进程自动终止
// ═══════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod job {
    use std::process::Child;

    /// 将子进程绑定到一个 Job Object，使其在主进程退出时自动被 Windows 内核终止。
    /// 这是防止僵尸进程的工业级方案（等同于 Chromium 的子进程管理策略）。
    pub fn bind_child_to_job(child: &Child) -> Result<(), String> {
        use windows_sys::Win32::System::JobObjects::*;
        use windows_sys::Win32::Foundation::CloseHandle;
        use std::mem;

        unsafe {
            // 1. 创建一个匿名 Job Object
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job == 0 {
                return Err("CreateJobObjectW failed".to_string());
            }

            // 2. 设置 JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            //    当最后一个 Job handle 关闭（即主进程退出）时，Windows 内核将自动终止所有子进程
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ret = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ret == 0 {
                CloseHandle(job);
                return Err("SetInformationJobObject failed".to_string());
            }

            // 3. 获取子进程句柄并绑定到 Job
            //    使用 PROCESS_SET_QUOTA | PROCESS_TERMINATE 的最小权限
            use windows_sys::Win32::System::Threading::OpenProcess;
            const PROCESS_SET_QUOTA: u32 = 0x0100;
            const PROCESS_TERMINATE: u32 = 0x0001;
            let process_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, child.id());
            if process_handle == 0 {
                CloseHandle(job);
                return Err("OpenProcess failed".to_string());
            }

            let ret = AssignProcessToJobObject(job, process_handle);
            CloseHandle(process_handle);
            
            if ret == 0 {
                CloseHandle(job);
                return Err("AssignProcessToJobObject failed".to_string());
            }

            // 注意：故意不关闭 job handle — 让它跟随主进程的生命周期。
            // 当主进程退出时，Windows 会自动关闭所有 handle，触发 KILL_ON_JOB_CLOSE。

            Ok(())
        }
    }
}

// 非 Windows 平台的 fallback（macOS/Linux 不需要 Job Object）
#[cfg(not(target_os = "windows"))]
mod job {
    use std::process::Child;
    pub fn bind_child_to_job(_child: &Child) -> Result<(), String> {
        Ok(()) // Unix 系统中子进程默认跟随父进程退出
    }
}

#[command]
pub async fn start_offline_engine(app: AppHandle, model_path: String) -> Result<Value, String> {
    // 1. 杀掉旧进程 (使用独立作用域防止锁跨越 await)
    {
        let state: State<SidecarState> = app.state();
        let mut child_lock = state.child.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut existing_child) = child_lock.take() {
            let _ = existing_child.kill();
        }
    }

    if model_path.is_empty() {
        return Err("模型路径不能为空".to_string());
    }

    // 解析资源目录路径
    let resource_dir = app.path().resolve("llm-engine", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("无法解析资源目录: {}", e))?;
        
    let exe_path = resource_dir.join("llama-server.exe");
    
    if !exe_path.exists() {
        return Err(format!("找不到执行文件: {}", exe_path.display()));
    }

    let child = Command::new(&exe_path)
        .current_dir(&resource_dir) // 必须设置工作目录，否则找不到旁边的 dll 文件
        .args(["-m", &model_path, "--port", "8080", "--ctx-size", "4096"])
        .spawn()
        .map_err(|e| format!("启动失败: {}", e))?;

    // 绑定子进程到 Job Object，确保"父死子必亡"
    if let Err(e) = job::bind_child_to_job(&child) {
        log::warn!("Failed to bind sidecar to Job Object: {}. Zombie process risk remains.", e);
    }

    // 2. 保存新进程 (同样使用独立作用域)
    {
        let state: State<SidecarState> = app.state();
        let mut child_lock = state.child.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *child_lock = Some(child);
    }

    // 轮询等待引擎启动完毕
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
        
    let mut ready = false;
    for _ in 0..20 { // 最多等待 20 秒 (对于大模型可能需要更久)
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Ok(res) = client.get("http://127.0.0.1:8080/health").send().await {
            if res.status().is_success() {
                ready = true;
                break;
            }
        }
    }

    if !ready {
        return Err("引擎启动超时，请检查控制台或终端日志".to_string());
    }

    Ok(json!({ "status": "running" }))
}

#[command]
pub async fn stop_offline_engine(app: AppHandle) -> Result<Value, String> {
    let state: State<SidecarState> = app.state();
    let mut child_lock = state.child.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(mut existing_child) = child_lock.take() {
        let _ = existing_child.kill();
        Ok(json!({ "status": "stopped" }))
    } else {
        Ok(json!({ "status": "already_stopped" }))
    }
}

#[command]
pub async fn get_offline_engine_status(app: AppHandle) -> Result<Value, String> {
    let state: State<SidecarState> = app.state();
    let child_lock = state.child.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    if child_lock.is_some() {
        Ok(json!({ "status": "running" }))
    } else {
        Ok(json!({ "status": "stopped" }))
    }
}

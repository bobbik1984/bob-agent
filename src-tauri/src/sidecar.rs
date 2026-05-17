use std::sync::Mutex;
use tauri::{AppHandle, State, command, Manager};
use std::process::{Command, Child};
use serde_json::{json, Value};

pub struct SidecarState {
    pub child: Mutex<Option<Child>>,
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

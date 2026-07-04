use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Emitter};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;
use reqwest::header::RANGE;
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
use tokio::sync::watch;

static ABORT_MAP: OnceLock<Mutex<HashMap<String, watch::Sender<bool>>>> = OnceLock::new();

fn get_abort_map() -> &'static Mutex<HashMap<String, watch::Sender<bool>>> {
    ABORT_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub progress: f64, // 0 to 100
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Serialize)]
pub struct DownloadResult {
    pub success: bool,
    pub path: String,
    pub error: Option<String>,
}

// 获取模型下载路径，区分移动端和桌面端
fn get_models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    // Tauri v2 统一获取 AppData 路径
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let models_dir = app_data.join("models");
    
    // 如果目录不存在，同步创建
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    }
    
    Ok(models_dir)
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_id: String,
    url: String,
    tokenizer_url: Option<String>,
) -> Result<DownloadResult, String> {
    let models_dir = get_models_dir(&app)?;
    let file_name = format!("{}.gguf", model_id);
    let file_path = models_dir.join(&file_name);
    let tmp_path = models_dir.join(format!("{}.download", file_name));

    // 如果已经下载完了，直接返回成功
    if file_path.exists() {
        return Ok(DownloadResult {
            success: true,
            path: file_path.to_string_lossy().to_string(),
            error: None,
        });
    }

    let client = reqwest::Client::new();
    
    // 检查是否有临时文件，用于断点续传
    let mut downloaded_bytes = 0u64;
    if tmp_path.exists() {
        if let Ok(metadata) = std::fs::metadata(&tmp_path) {
            downloaded_bytes = metadata.len();
        }
    }

    // 发起请求，带上 Range 头部
    let req = client.get(&url);
    let req = if downloaded_bytes > 0 {
        req.header(RANGE, format!("bytes={}-", downloaded_bytes))
    } else {
        req
    };

    let response = req.send().await.map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let total_bytes = response
        .content_length()
        .map(|len| len + downloaded_bytes)
        .unwrap_or(downloaded_bytes);

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&tmp_path)
        .await
        .map_err(|e| format!("Failed to open tmp file: {}", e))?;

    let (abort_tx, mut abort_rx) = watch::channel(false);
    {
        let mut map = get_abort_map().lock().unwrap();
        map.insert(model_id.clone(), abort_tx);
    }

    let mut stream = response.bytes_stream();
    let mut current_bytes = downloaded_bytes;
    let mut last_progress = -1.0;
    let mut paused = false;

    loop {
        tokio::select! {
            chunk_result = stream.next() => {
                match chunk_result {
                    Some(Ok(chunk)) => {
                        file.write_all(&chunk).await.map_err(|e| format!("Error writing file: {}", e))?;
                        current_bytes += chunk.len() as u64;

                        if total_bytes > 0 {
                            let progress = (current_bytes as f64 / total_bytes as f64) * 100.0;
                            if progress - last_progress >= 0.5 || progress == 100.0 {
                                last_progress = progress;
                                let _ = app.emit("download_progress", DownloadProgress {
                                    model_id: model_id.clone(),
                                    progress,
                                    downloaded_bytes: current_bytes,
                                    total_bytes,
                                });
                            }
                        }
                    }
                    Some(Err(e)) => return Err(format!("Error reading stream: {}", e)),
                    None => break, // Stream finished
                }
            }
            _ = abort_rx.changed() => {
                if *abort_rx.borrow() {
                    paused = true;
                    break; // Aborted by user
                }
            }
        }
    }

    // 清理 abort map
    {
        let mut map = get_abort_map().lock().unwrap();
        map.remove(&model_id);
    }

    // 确保写入完毕
    file.flush().await.map_err(|e| format!("Failed to flush: {}", e))?;
    
    // 强制关闭文件句柄
    drop(file);

    if paused {
        return Ok(DownloadResult {
            success: false,
            path: tmp_path.to_string_lossy().to_string(),
            error: Some("Paused".to_string()),
        });
    }

    // 下载完成，重命名 GGUF
    tokio::fs::rename(&tmp_path, &file_path).await.map_err(|e| format!("Failed to rename: {}", e))?;

    // 下载 Tokenizer
    if let Some(t_url) = tokenizer_url {
        let t_file_name = format!("{}_tokenizer.json", model_id);
        let t_file_path = models_dir.join(&t_file_name);
        if !t_file_path.exists() {
            let t_response = client.get(&t_url).send().await.map_err(|e| format!("Tokenizer request failed: {}", e))?;
            if t_response.status().is_success() {
                let t_bytes = t_response.bytes().await.map_err(|e| format!("Tokenizer body error: {}", e))?;
                tokio::fs::write(&t_file_path, &t_bytes).await.map_err(|e| format!("Failed to write tokenizer: {}", e))?;
            }
        }
    }

    // 推送完成事件
    let _ = app.emit("download_progress", DownloadProgress {
        model_id: model_id.clone(),
        progress: 100.0,
        downloaded_bytes: total_bytes,
        total_bytes,
    });

    Ok(DownloadResult {
        success: true,
        path: file_path.to_string_lossy().to_string(),
        error: None,
    })
}

#[tauri::command]
pub async fn check_model_downloaded(app: AppHandle, model_id: String) -> Result<bool, String> {
    let models_dir = get_models_dir(&app)?;
    let file_name = format!("{}.gguf", model_id);
    let file_path = models_dir.join(&file_name);
    Ok(file_path.exists())
}

#[tauri::command]
pub fn pause_download(model_id: String) {
    let mut map = get_abort_map().lock().unwrap();
    if let Some(tx) = map.get(&model_id) {
        let _ = tx.send(true);
    }
}

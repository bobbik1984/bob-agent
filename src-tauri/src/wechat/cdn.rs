//! WeChat CDN 媒体上传管线
//!
//! 完整流程：读取本地文件 → MD5 哈希 → AES-128-ECB 加密 → getUploadUrl → POST to CDN → 返回上传结果。
//! 参考实现：wechat_bot/src/cdn/upload.ts + cdn-upload.ts

use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use aes::Aes128;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use md5::{Digest, Md5};
use rand::Rng;
use std::path::Path;
use tauri::Emitter;

use super::api::WechatApi;
use super::types::*;

/// CDN 上传后返回的信息，用于构造 MessageItem
#[derive(Debug, Clone)]
pub struct UploadedFileInfo {
    pub filekey: String,
    /// CDN 返回的 x-encrypted-param，填入 CdnMedia.encrypt_query_param
    pub download_encrypted_query_param: String,
    /// AES-128 key, hex 编码 (转 base64 后填入 CdnMedia.aes_key)
    pub aeskey_hex: String,
    /// 原文件明文大小 (bytes)
    pub file_size: u64,
    /// 密文大小 (bytes), AES-128-ECB PKCS7
    pub file_size_ciphertext: u64,
}

// ═══════════════════════════════════════════════════════════
// AES-128-ECB 加密
// ═══════════════════════════════════════════════════════════

/// 计算 AES-128-ECB PKCS7 padding 后的密文大小
fn aes_ecb_padded_size(plaintext_size: u64) -> u64 {
    // PKCS7 padding: 总是添加 1-16 字节, 对齐到 16 字节边界
    ((plaintext_size + 1 + 15) / 16) * 16
}

/// AES-128-ECB 加密 (手动实现 PKCS7 padding)
/// 等价于 Node.js 的 createCipheriv("aes-128-ecb", key, null)
fn encrypt_aes_128_ecb(plaintext: &[u8], key: &[u8; 16]) -> Vec<u8> {
    // PKCS7 padding
    let block_size = 16usize;
    let pad_len = block_size - (plaintext.len() % block_size);
    let mut padded = Vec::with_capacity(plaintext.len() + pad_len);
    padded.extend_from_slice(plaintext);
    padded.resize(plaintext.len() + pad_len, pad_len as u8);

    // ECB encryption: encrypt each 16-byte block independently
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut output = vec![0u8; padded.len()];
    for (i, chunk) in padded.chunks(block_size).enumerate() {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        output[i * block_size..(i + 1) * block_size].copy_from_slice(&block);
    }
    output
}

// ═══════════════════════════════════════════════════════════
// CDN URL 构造
// ═══════════════════════════════════════════════════════════

/// 拼接 CDN 上传 URL (当服务端未返回 upload_full_url 时使用)
fn build_cdn_upload_url(cdn_base_url: &str, upload_param: &str, filekey: &str) -> String {
    format!(
        "{}/upload?encrypted_query_param={}&filekey={}",
        cdn_base_url,
        urlencoding::encode(upload_param),
        urlencoding::encode(filekey)
    )
}

// ═══════════════════════════════════════════════════════════
// 上传管线
// ═══════════════════════════════════════════════════════════

const UPLOAD_MAX_RETRIES: u32 = 3;

/// 构建带进度回调的 reqwest Body：将 ciphertext 按 CHUNK_SIZE 分块，每块发送后通过 AppHandle emit 进度事件
const UPLOAD_CHUNK_SIZE: usize = 65_536; // 64KB per chunk

fn build_progress_body(
    ciphertext: Vec<u8>,
    app: tauri::AppHandle,
    file_name: String,
    attempt: u32,
) -> reqwest::Body {
    let total = ciphertext.len();
    let stream = futures::stream::unfold((ciphertext, 0usize), move |(data, offset)| {
        let app = app.clone();
        let file_name = file_name.clone();
        async move {
            if offset >= data.len() {
                return None;
            }
            let end = std::cmp::min(offset + UPLOAD_CHUNK_SIZE, data.len());
            let chunk = data[offset..end].to_vec();
            let sent = end;
            let percent = (sent as f64 / total as f64 * 100.0).round() as u32;
            let _ = app.emit(
                "cdn:upload-progress",
                serde_json::json!({
                    "file_name": &file_name,
                    "bytes_sent": sent,
                    "total_bytes": total,
                    "percent": percent,
                    "attempt": attempt,
                }),
            );
            Some((
                Ok::<_, std::io::Error>(bytes::Bytes::from(chunk)),
                (data, end),
            ))
        }
    });
    reqwest::Body::wrap_stream(stream)
}

/// POST 加密后的文件到微信 CDN, 返回 download_encrypted_query_param
async fn upload_buffer_to_cdn(
    ciphertext: &[u8],
    upload_full_url: Option<&str>,
    upload_param: Option<&str>,
    filekey: &str,
    app: &tauri::AppHandle,
    file_name: &str,
) -> Result<String, String> {
    let cdn_url = if let Some(full_url) = upload_full_url {
        let trimmed = full_url.trim();
        if !trimmed.is_empty() {
            trimmed.to_string()
        } else if let Some(param) = upload_param {
            build_cdn_upload_url(CDN_BASE_URL, param, filekey)
        } else {
            return Err(
                "CDN upload URL missing (need upload_full_url or upload_param)".to_string(),
            );
        }
    } else if let Some(param) = upload_param {
        build_cdn_upload_url(CDN_BASE_URL, param, filekey)
    } else {
        return Err("CDN upload URL missing (need upload_full_url or upload_param)".to_string());
    };

    // 根据文件大小动态计算超时：每 MB 给 30 秒，最低 120 秒
    let size_mb = (ciphertext.len() as u64 + 1_048_575) / 1_048_576;
    let timeout_secs = std::cmp::max(120, size_mb * 30);
    log::info!(
        "[cdn] POST to CDN (size={}MB, timeout={}s)",
        size_mb,
        timeout_secs
    );

    // 通知前端上传开始
    let _ = app.emit(
        "cdn:upload-start",
        serde_json::json!({
            "file_name": file_name,
            "total_bytes": ciphertext.len(),
        }),
    );

    let mut last_error: Option<String> = None;

    for attempt in 1..=UPLOAD_MAX_RETRIES {
        log::info!(
            "[cdn] upload attempt {}/{} ...",
            attempt,
            UPLOAD_MAX_RETRIES
        );

        // 每次重试都需要重新构建 body（stream 只能消费一次）
        let body = build_progress_body(
            ciphertext.to_vec(),
            app.clone(),
            file_name.to_string(),
            attempt,
        );

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/octet-stream"),
        );

        let res = crate::tunnel::send_request(
            reqwest::Method::POST,
            &cdn_url,
            headers,
            Some(body),
            std::time::Duration::from_secs(timeout_secs),
        )
        .await;

        match res {
            Ok(response) => {
                let status = response.status().as_u16();
                if status >= 400 && status < 500 {
                    let err_msg = response
                        .headers()
                        .get("x-error-message")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("client error")
                        .to_string();
                    let _ = app.emit(
                        "cdn:upload-error",
                        serde_json::json!({
                            "file_name": file_name, "error": &err_msg,
                        }),
                    );
                    return Err(format!("CDN upload client error {}: {}", status, err_msg));
                }
                if status != 200 {
                    let err_msg = format!("CDN upload server error: status {}", status);
                    last_error = Some(err_msg.clone());
                    log::error!(
                        "[cdn] attempt {}/{} failed: {}",
                        attempt,
                        UPLOAD_MAX_RETRIES,
                        err_msg
                    );
                    continue;
                }

                // 从响应头取 x-encrypted-param
                let download_param = response
                    .headers()
                    .get("x-encrypted-param")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                if let Some(param) = download_param {
                    log::info!("[cdn] upload success on attempt {}", attempt);
                    let _ = app.emit(
                        "cdn:upload-done",
                        serde_json::json!({
                            "file_name": file_name, "attempt": attempt,
                        }),
                    );
                    return Ok(param);
                } else {
                    last_error = Some("CDN response missing x-encrypted-param header".to_string());
                    log::error!(
                        "[cdn] attempt {}/{} missing x-encrypted-param",
                        attempt,
                        UPLOAD_MAX_RETRIES
                    );
                    continue;
                }
            }
            Err(e) => {
                last_error = Some(format!("CDN upload request failed: {}", e));
                log::error!(
                    "[cdn] attempt {}/{} failed: {}",
                    attempt,
                    UPLOAD_MAX_RETRIES,
                    e
                );
            }
        }
    }

    let final_err = last_error
        .unwrap_or_else(|| format!("CDN upload failed after {} attempts", UPLOAD_MAX_RETRIES));
    let _ = app.emit(
        "cdn:upload-error",
        serde_json::json!({
            "file_name": file_name, "error": &final_err,
        }),
    );
    Err(final_err)
}

/// 完整的媒体上传管线：读取文件 → 计算 MD5 → 生成 AES key → getUploadUrl → 加密上传 → 返回结果
///
/// # Arguments
/// * `api` - WechatApi 实例 (含 base_url 和 token)
/// * `file_path` - 本地文件的绝对路径
/// * `to_user_id` - 目标微信用户 ID (wxid)
/// * `media_type` - UPLOAD_MEDIA_TYPE_IMAGE / UPLOAD_MEDIA_TYPE_FILE
pub async fn upload_media(
    api: &WechatApi,
    file_path: &str,
    to_user_id: &str,
    media_type: i32,
    app: &tauri::AppHandle,
) -> Result<UploadedFileInfo, String> {
    let path = Path::new(file_path);

    // 1. 校验文件存在性和大小
    if !path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }

    let metadata = std::fs::metadata(path).map_err(|e| format!("无法读取文件元数据: {}", e))?;

    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "文件大小 ({:.1}MB) 超过上限 (100MB)",
            metadata.len() as f64 / 1024.0 / 1024.0
        ));
    }

    if metadata.len() == 0 {
        return Err("文件为空 (0 字节)".to_string());
    }

    // 2. 读取文件内容
    let plaintext = std::fs::read(path).map_err(|e| format!("无法读取文件: {}", e))?;

    let rawsize = plaintext.len() as u64;
    let filesize = aes_ecb_padded_size(rawsize);

    // 3. 计算 MD5
    let mut hasher = Md5::new();
    hasher.update(&plaintext);
    let rawfilemd5 = format!("{:x}", hasher.finalize());

    // 4. 生成随机 AES-128 key 和 filekey
    //    注意：thread_rng() 返回 !Send 类型，必须在 block 内使用并 drop，
    //    否则 async fn 的 future 变为 !Send，无法被 tokio::spawn。
    let (aeskey, aeskey_hex, filekey) = {
        let mut rng = rand::thread_rng();
        let mut aeskey = [0u8; 16];
        rng.fill(&mut aeskey);
        let aeskey_hex = hex_encode(&aeskey);

        let mut filekey_bytes = [0u8; 16];
        rng.fill(&mut filekey_bytes);
        let filekey = hex_encode(&filekey_bytes);
        (aeskey, aeskey_hex, filekey)
    };

    log::info!(
        "[cdn] upload_media: file={} rawsize={} filesize={} md5={} media_type={}",
        file_path,
        rawsize,
        filesize,
        rawfilemd5,
        media_type
    );

    let fileext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_string();

    // 5. 调用 getUploadUrl API
    let upload_url_req = GetUploadUrlReq {
        filekey: Some(filekey.clone()),
        fileext: Some(fileext),
        media_type: Some(media_type),
        to_user_id: Some(to_user_id.to_string()),
        rawsize: Some(rawsize),
        rawfilemd5: Some(rawfilemd5),
        filesize: Some(filesize),
        no_need_thumb: Some(true),
        aeskey: Some(aeskey_hex.clone()),
        base_info: None, // api.get_upload_url 会自动填充
    };

    let upload_url_resp = api.get_upload_url(upload_url_req, 15_000).await?;

    let upload_full_url = upload_url_resp.upload_full_url.as_deref();
    let upload_param = upload_url_resp.upload_param.as_deref();

    if upload_full_url.is_none() && upload_param.is_none() {
        let errcode = upload_url_resp.errcode.unwrap_or(0);
        let errmsg = upload_url_resp.errmsg.unwrap_or_default();
        return Err(format!(
            "getUploadUrl returned no upload URL (ret={:?}, errcode={}, errmsg={})",
            upload_url_resp.ret, errcode, errmsg
        ));
    }

    // 6. AES-128-ECB 加密文件内容
    let ciphertext = encrypt_aes_128_ecb(&plaintext, &aeskey);
    log::debug!(
        "[cdn] encrypted: plaintext={} ciphertext={}",
        plaintext.len(),
        ciphertext.len()
    );

    // 7. POST 到 CDN（带实时进度推送）
    let display_name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let download_encrypted_query_param = upload_buffer_to_cdn(
        &ciphertext,
        upload_full_url,
        upload_param,
        &filekey,
        app,
        &display_name,
    )
    .await?;

    Ok(UploadedFileInfo {
        filekey,
        download_encrypted_query_param,
        aeskey_hex,
        file_size: rawsize,
        file_size_ciphertext: filesize,
    })
}

/// 判断文件扩展名是否为图片格式
pub fn is_image_file(file_path: &str) -> bool {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "ico" | "svg" | "tiff" | "tif"
    )
}

/// 将字节数组编码为十六进制字符串
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_ecb_padded_size() {
        assert_eq!(aes_ecb_padded_size(0), 16);
        assert_eq!(aes_ecb_padded_size(1), 16);
        assert_eq!(aes_ecb_padded_size(15), 16);
        assert_eq!(aes_ecb_padded_size(16), 32); // 16 bytes + at least 1 padding byte → 32
        assert_eq!(aes_ecb_padded_size(17), 32);
        assert_eq!(aes_ecb_padded_size(31), 32);
        assert_eq!(aes_ecb_padded_size(32), 48);
    }

    #[test]
    fn test_encrypt_aes_128_ecb_roundtrip() {
        let key = [0x01u8; 16];
        let plaintext = b"Hello, WeChat CDN!";
        let ciphertext = encrypt_aes_128_ecb(plaintext, &key);
        assert_eq!(ciphertext.len(), 32); // 18 bytes → pad to 32
                                          // Ciphertext should not be all zeros
        assert!(ciphertext.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_is_image_file() {
        assert!(is_image_file("photo.jpg"));
        assert!(is_image_file("icon.PNG"));
        assert!(is_image_file("pic.webp"));
        assert!(!is_image_file("doc.pdf"));
        assert!(!is_image_file("video.mp4"));
        assert!(!is_image_file("readme.txt"));
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0xab, 0xcd, 0xef]), "abcdef");
        assert_eq!(hex_encode(&[0x00, 0xff]), "00ff");
    }
}

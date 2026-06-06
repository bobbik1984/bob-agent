//! browser.rs — 浏览器增强模块
//!
//! 使用 chromiumoxide 通过 CDP 协议控制本机 Edge/Chrome，
//! 为 Bob 的 LLM 工具链提供 `browse_page` 能力。
//!
//! 设计原则：
//! - 懒加载：首次调用时才启动浏览器
//! - 静默：headless 模式，不弹窗
//! - 隐私：URL 参数脱敏、日志不记录页面内容
//! - 自动回收：空闲 5 分钟关闭浏览器进程

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use log::{info, warn, debug};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::ScreenshotParams;
use futures_util::StreamExt;

/// 浏览器全局状态
pub struct BrowserState {
    browser: Mutex<Option<BrowserInstance>>,
}

struct BrowserInstance {
    browser: Browser,
    last_used: Instant,
    browser_path: PathBuf,
}

/// 空闲超时（5 分钟）
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// 单页最长加载时间
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

impl BrowserState {
    pub fn new() -> Self {
        Self {
            browser: Mutex::new(None),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 隐私脱敏 helpers
// ═══════════════════════════════════════════════════════════

/// URL 查询参数脱敏：保留域名+路径，隐藏 query string
fn sanitize_url(url: &str) -> String {
    match url.find('?') {
        Some(pos) => format!("{}?***", &url[..pos]),
        None => {
            // 也隐藏 fragment
            match url.find('#') {
                Some(pos) => format!("{}#***", &url[..pos]),
                None => url.to_string(),
            }
        }
    }
}

/// 提取 URL 的域名部分用于日志
fn extract_domain(url: &str) -> &str {
    let without_scheme = url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    without_scheme.split('/').next().unwrap_or(url)
}

// ═══════════════════════════════════════════════════════════
// 浏览器检测
// ═══════════════════════════════════════════════════════════

/// 检测本机 Edge/Chrome 路径（Windows 优先 Edge）
pub fn detect_browser() -> Option<PathBuf> {
    // 先检查 config 中用户手动指定的路径
    let config = crate::read_config();
    if let Some(path_str) = config.get("browserPath").and_then(|v| v.as_str()) {
        let p = PathBuf::from(path_str);
        if p.exists() {
            info!("[browser] using configured path: {}", p.display());
            return Some(p);
        }
    }

    // Windows 默认路径（优先 Edge）
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];

    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            info!("[browser] detected: {}", p.display());
            return Some(p);
        }
    }

    // 尝试 Windows 注册表
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = detect_from_registry() {
            return Some(p);
        }
    }

    warn!("[browser] no Edge/Chrome found on this system");
    None
}

#[cfg(target_os = "windows")]
fn detect_from_registry() -> Option<PathBuf> {
    use std::process::Command;
    // 查询注册表中 Edge 的安装路径
    let output = Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\msedge.exe", "/ve"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("REG_SZ") {
            let path_str = line.split("REG_SZ").last()?.trim();
            let p = PathBuf::from(path_str);
            if p.exists() {
                info!("[browser] detected from registry: {}", p.display());
                return Some(p);
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════
// 浏览器启动与管理
// ═══════════════════════════════════════════════════════════

/// 获取或启动浏览器实例（懒加载）
async fn get_or_launch(state: &BrowserState) -> Result<(), String> {
    let mut guard = state.browser.lock().await;
    
    // 检查现有实例是否还活着
    if let Some(ref mut instance) = *guard {
        instance.last_used = Instant::now();
        return Ok(());
    }

    // 需要启动新实例
    let browser_path = detect_browser()
        .ok_or_else(|| "未检测到 Edge 或 Chrome 浏览器。请安装 Microsoft Edge 后重试。".to_string())?;

    info!("[browser] launching headless: {}", browser_path.display());

    let user_data_dir = crate::get_data_dir().join("browser-profile");
    let _ = std::fs::create_dir_all(&user_data_dir);

    let config = BrowserConfig::builder()
        .chrome_executable(browser_path.clone())
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--disable-logging")
        .arg("--silent-launch")
        .arg("--no-first-run")
        .arg("--disable-extensions")
        .arg("--disable-default-apps")
        .arg("--disable-translate")
        .arg("--disable-sync")
        .arg("--disable-background-networking")
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .build()
        .map_err(|e| format!("浏览器配置构建失败: {}", e))?;

    let (browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| format!("浏览器启动失败: {}", e))?;

    // 在后台驱动 CDP 消息循环
    tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                debug!("[browser] handler event error, stopping loop");
                break;
            }
        }
        debug!("[browser] handler loop ended");
    });

    info!("[browser] headless browser ready");

    *guard = Some(BrowserInstance {
        browser,
        last_used: Instant::now(),
        browser_path,
    });

    Ok(())
}

/// 启动空闲检查任务（在 setup 中调用一次）
pub fn start_idle_watcher(state: Arc<BrowserState>) {
    // 必须使用 tauri::async_runtime::spawn 而非 tokio::spawn，
    // 因为此函数从 Tauri 的同步 setup 回调中调用，主线程上没有 Tokio reactor
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let mut guard = state.browser.lock().await;
            if let Some(ref instance) = *guard {
                if instance.last_used.elapsed() > IDLE_TIMEOUT {
                    info!("[browser] idle timeout, shutting down browser process");
                    *guard = None; // Drop closes the browser
                }
            }
        }
    });
}

/// 优雅关闭浏览器
pub async fn shutdown(state: &BrowserState) {
    let mut guard = state.browser.lock().await;
    if guard.is_some() {
        *guard = None;
        info!("[browser] shutdown complete");
    }
}

// ═══════════════════════════════════════════════════════════
// 核心工具：browse_page
// ═══════════════════════════════════════════════════════════

/// 使用本机浏览器打开网页并提取内容
///
/// # 参数
/// - `url`: 要访问的网页 URL
/// - `wait_seconds`: 等待页面加载的秒数（默认 3）
/// - `click_selector`: 可选，加载后点击的 CSS 选择器
/// - `extract`: 提取模式 "text" | "html" | "screenshot"
pub async fn browse_page(
    state: &BrowserState,
    url: &str,
    wait_seconds: u64,
    click_selector: Option<&str>,
    extract: &str,
) -> Result<String, String> {
    let start = Instant::now();
    let domain = extract_domain(url);
    info!("[browser] navigate domain={} wait={}s extract={}", domain, wait_seconds, extract);

    // 确保浏览器已启动
    get_or_launch(state).await?;

    let guard = state.browser.lock().await;
    let instance = guard.as_ref().ok_or("浏览器实例不可用")?;
    
    // 打开新页面
    let page = instance.browser.new_page(url)
        .await
        .map_err(|e| format!("打开页面失败: {}", e))?;

    // 等待页面加载
    let wait_ms = (wait_seconds * 1000).min(PAGE_TIMEOUT.as_millis() as u64);
    tokio::time::sleep(Duration::from_millis(wait_ms)).await;

    // 可选：点击元素（如「展开全文」按钮）
    if let Some(selector) = click_selector {
        debug!("[browser] clicking selector: {}", selector);
        match page.find_element(selector).await {
            Ok(element) => {
                if let Err(e) = element.click().await {
                    warn!("[browser] click failed: {}", e);
                }
                // 点击后等待 1 秒加载
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                debug!("[browser] selector not found: {} ({})", selector, e);
            }
        }
    }

    // 提取内容
    let result = match extract {
        "html" => {
            let content = page.content()
                .await
                .map_err(|e| format!("提取 HTML 失败: {}", e))?;
            let char_count = content.chars().count();
            // 限制返回长度避免 LLM 上下文爆炸
            let truncated = if char_count > 15000 {
                let t: String = content.chars().take(15000).collect();
                format!("{}...\n[HTML 已截断，原始长度: {} 字符]", t, char_count)
            } else {
                content
            };
            info!("[browser] extracted html chars={} elapsed={:.1}s", char_count, start.elapsed().as_secs_f64());
            truncated
        }
        "screenshot" => {
            let screenshot_data = page.screenshot(ScreenshotParams::builder().build())
                .await
                .map_err(|e| format!("截图失败: {}", e))?;
            let b64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &screenshot_data,
            );
            info!("[browser] screenshot bytes={} elapsed={:.1}s", screenshot_data.len(), start.elapsed().as_secs_f64());
            format!("data:image/png;base64,{}", b64)
        }
        _ => {
            // 默认提取纯文本（使用 innerText）
            let text_result = page.evaluate("document.body.innerText")
                .await
                .map_err(|e| format!("提取文本失败: {}", e))?;
            
            let text = text_result.into_value::<String>()
                .unwrap_or_default();
            
            let char_count = text.chars().count();
            let truncated = if char_count > 8000 {
                let t: String = text.chars().take(8000).collect();
                format!("{}...\n[文本已截断，原始长度: {} 字符]", t, char_count)
            } else {
                text
            };
            info!("[browser] extracted text chars={} elapsed={:.1}s", char_count, start.elapsed().as_secs_f64());
            truncated
        }
    };

    // 关闭页面释放资源
    let _ = page.close().await;

    // 更新最后使用时间
    drop(guard);
    {
        let mut guard = state.browser.lock().await;
        if let Some(ref mut instance) = *guard {
            instance.last_used = Instant::now();
        }
    }

    Ok(result)
}

/// 检查浏览器增强是否已启用
pub fn is_browser_enabled() -> bool {
    let config = crate::read_config();
    config.get("browserEnhanced")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// 启用浏览器增强（写入 config）
pub fn enable_browser() {
    let mut config = crate::read_config();
    if let Some(obj) = config.as_object_mut() {
        obj.insert("browserEnhanced".to_string(), serde_json::json!(true));
        // 同时检测并记录浏览器路径
        if let Some(path) = detect_browser() {
            obj.insert("browserPath".to_string(), serde_json::json!(path.to_string_lossy().to_string()));
        }
    }
    crate::write_config(&config);
    info!("[browser] browser enhancement enabled by user");
}

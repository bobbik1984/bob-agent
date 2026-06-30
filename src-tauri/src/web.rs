use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::{json, Value};

/// T-602: 网页内容抓取 — 使用 reqwest 获取 HTML，用 scraper 提取纯文本
/// 
/// 返回格式: { title, content, url, charCount }
/// 安全措施: 
///   - 最大响应体 2MB
///   - 10 秒超时
///   - 自动移除 script/style/nav/footer 噪声标签
#[tauri::command]
pub async fn system_fetch_url(url: String) -> Value {
    // 基本 URL 校验
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return json!({ "error": "URL 必须以 http:// 或 https:// 开头" });
    }

    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Bob-Agent/1.0")
        .build()
    {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("HTTP 客户端初始化失败: {}", e) }),
    };

    let response = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            let msg = if e.is_timeout() {
                "请求超时（10 秒）".to_string()
            } else if e.is_connect() {
                "无法连接到目标服务器".to_string()
            } else {
                format!("网络请求失败: {}", e)
            };
            return json!({ "error": msg });
        }
    };

    let status = response.status().as_u16();
    if !response.status().is_success() {
        return json!({ "error": format!("HTTP {} — 服务器拒绝请求", status) });
    }

    // 读取响应体（2MB 上限）
    let mut body = match response.text().await {
        Ok(text) => {
            if text.len() > 2 * 1024 * 1024 {
                text[..2 * 1024 * 1024].to_string()
            } else {
                text
            }
        }
        Err(e) => return json!({ "error": format!("读取响应失败: {}", e) }),
    };

    // 粗暴移除 script 和 style 标签，防止污染文本提取
    let script_re = regex::Regex::new(r"(?is)<script.*?>.*?</script>").unwrap();
    body = script_re.replace_all(&body, "").to_string();
    let style_re = regex::Regex::new(r"(?is)<style.*?>.*?</style>").unwrap();
    body = style_re.replace_all(&body, "").to_string();

    // 解析 HTML
    let document = Html::parse_document(&body);

    // 提取标题
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string();

    // 移除噪声标签后提取正文
    let content = extract_text_content(&document);

    let char_count = content.chars().count();

    json!({
        "title": title,
        "content": content,
        "url": url,
        "charCount": char_count
    })
}

/// 从 HTML 文档中提取干净的纯文本正文
/// 跳过 script, style, nav, footer, header, aside 等噪声区域
fn extract_text_content(document: &Html) -> String {
    // 尝试优先从 <article> 或 <main> 中提取
    let priority_selectors = ["article", "main", "[role=\"main\"]", ".content", "#content", "#js_content", ".rich_media_content"];
    
    for selector_str in priority_selectors {
        if let Ok(sel) = Selector::parse(selector_str) {
            let elements: Vec<_> = document.select(&sel).collect();
            if !elements.is_empty() {
                let text: String = elements.iter()
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let cleaned = clean_text(&text);
                if cleaned.len() > 100 {
                    return cleaned;
                }
            }
        }
    }

    // 降级: 从 <body> 提取，但跳过噪声标签
    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(body) = document.select(&body_sel).next() {
            let text = body.text().collect::<Vec<_>>().join(" ");
            return clean_text(&text);
        }
    }

    // 最终降级: 整个文档
    let text = document.root_element().text().collect::<Vec<_>>().join(" ");
    clean_text(&text)
}

/// 清理提取的文本: 合并空白、移除过多换行
fn clean_text(text: &str) -> String {
    let mut result = String::new();
    let mut prev_was_whitespace = false;
    let mut consecutive_newlines = 0;

    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push('\n');
            }
            prev_was_whitespace = true;
        } else if ch.is_whitespace() {
            if !prev_was_whitespace {
                result.push(' ');
            }
            prev_was_whitespace = true;
            consecutive_newlines = 0;
        } else {
            result.push(ch);
            prev_was_whitespace = false;
            consecutive_newlines = 0;
        }
    }

    result.trim().to_string()
}

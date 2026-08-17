use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportData {
    pub title: String,
    pub template: String,
    pub content: String,
}

pub fn generate_html_report(data: &ReportData) -> String {
    // 基础的 HTML 骨架，后续可以支持多种模板 (Corporate, Academic, Dashboard)
    // 目前使用类似 o2_analysis 的默认风格
    let css = r#"
        :root {
            --bg-root: #f8f9fa;
            --bg-surface: #ffffff;
            --text-primary: #111827;
            --text-secondary: #4b5563;
            --border-color: #e5e7eb;
            --accent-primary: #2563eb;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            background-color: var(--bg-root);
            color: var(--text-primary);
            line-height: 1.6;
            margin: 0;
            padding: 2rem;
        }
        .report-container {
            max-width: 800px;
            margin: 0 auto;
            background: var(--bg-surface);
            padding: 3rem;
            border-radius: 8px;
            box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
        }
        h1 { color: var(--text-primary); border-bottom: 2px solid var(--accent-primary); padding-bottom: 0.5rem; margin-top: 0; }
        h2 { color: var(--text-primary); margin-top: 2rem; }
        p { color: var(--text-secondary); }
        .markdown-content img { max-width: 100%; height: auto; border-radius: 4px; }
        .markdown-content table { width: 100%; border-collapse: collapse; margin-top: 1rem; margin-bottom: 1rem; }
        .markdown-content th, .markdown-content td { border: 1px solid var(--border-color); padding: 0.5rem; text-align: left; }
        .markdown-content th { background-color: #f3f4f6; }
        .markdown-content pre { background: #f3f4f6; padding: 1rem; border-radius: 4px; overflow-x: auto; }
        
        @media print {
            body { background: transparent; padding: 0; }
            .report-container { box-shadow: none; max-width: 100%; padding: 0; }
            h2 { page-break-after: avoid; }
            img { max-width: 100% !important; page-break-inside: avoid; }
            /* Force print background colors */
            * { -webkit-print-color-adjust: exact !important; color-adjust: exact !important; }
        }
    "#;

    // TODO: 目前 content 是直接接收 markdown。
    // 在生产环境中，我们应该在 rust 端用 pulldown-cmark 将其转换为 HTML，
    // 或者引入一个轻量级的 JS 库（如 marked.js）在浏览器端渲染。
    // 这里我们先使用一个内嵌的 marked.js 脚本来进行客户端渲染，以保证 Markdown 的完整支持。

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
    <style>{css}</style>
</head>
<body>
    <div class="report-container">
        <h1>{title}</h1>
        <div id="content" class="markdown-content"></div>
    </div>
    
    <!-- Action buttons for standard viewing (hidden when printing) -->
    <div class="actions" style="position: fixed; top: 20px; right: 20px;">
        <button onclick="window.print()" style="padding: 10px 20px; background: #2563eb; color: white; border: none; border-radius: 4px; cursor: pointer; font-weight: bold; box-shadow: 0 2px 4px rgba(0,0,0,0.1);">
            <svg style="width:16px;height:16px;vertical-align:text-bottom;margin-right:4px" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 17h2a2 2 0 002-2v-4a2 2 0 00-2-2H5a2 2 0 00-2 2v4a2 2 0 002 2h2m2 4h6a2 2 0 002-2v-4a2 2 0 00-2-2H9a2 2 0 00-2 2v4a2 2 0 002 2zm8-12V5a2 2 0 00-2-2H9a2 2 0 00-2 2v4h10z"></path></svg>
            导出 PDF
        </button>
    </div>
    <style>@media print {{ .actions {{ display: none !important; }} }}</style>

    <script>
        // Use marked.js to parse the raw markdown content embedded in the template
        const rawContent = `{content}`;
        document.getElementById('content').innerHTML = marked.parse(rawContent);
    </script>
</body>
</html>"#,
        title = data.title.replace("`", "\\`"),
        css = css,
        // 防止由于字符串插值破坏 JS 语法，需要对 markdown 里的特殊字符做一点转义
        content = data.content.replace("`", "\\`").replace("$", "\\$")
    )
}

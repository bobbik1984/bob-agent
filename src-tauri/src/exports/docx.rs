use docx_rs::*;
use std::path::Path;
use std::fs::File;

pub fn generate_docx<P: AsRef<Path>>(path: P, raw_markdown: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.as_ref();
    
    // 我们目前实现一个非常基础的 markdown 到 docx 的映射
    // 生产环境中应该使用 pulldown-cmark 进行完整 AST 解析
    // 这里做最简单的换行分段和 # 标题解析
    
    let mut doc = Docx::new();
    
    // 设置页面
    doc = doc.page_size(595 * 20, 842 * 20) // A4: 210x297mm (Twips)
             .page_margin(
                 PageMargin::new()
                    .top(1440) // 1 inch
                    .bottom(1440)
                    .left(1440)
                    .right(1440)
             );

    let lines = raw_markdown.lines();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut p = Paragraph::new();

        if trimmed.starts_with("# ") {
            p = p.style("Heading1")
                 .add_run(Run::new().add_text(trimmed[2..].trim()).size(48).bold()); // 24pt
        } else if trimmed.starts_with("## ") {
            p = p.style("Heading2")
                 .add_run(Run::new().add_text(trimmed[3..].trim()).size(36).bold()); // 18pt
        } else if trimmed.starts_with("### ") {
            p = p.style("Heading3")
                 .add_run(Run::new().add_text(trimmed[4..].trim()).size(28).bold()); // 14pt
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            // 非常基础的列表支持
            p = p.add_run(Run::new().add_text(trimmed)) // TODO: 应该用真正的 Numbering
                 .indent(Some(720), None, None, None); // 缩进 0.5 inch
        } else {
            // 普通段落
            p = p.add_run(Run::new().add_text(trimmed).size(24)); // 12pt
        }
        
        doc = doc.add_paragraph(p);
    }

    let file = File::create(path)?;
    doc.build().pack(file)?;

    Ok(())
}

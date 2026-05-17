use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// LLM-Wiki 知识库引擎 — Phase A: 原生文件解析器
///
/// 职责: 将用户磁盘上的二进制文件格式暴力转为纯 UTF-8 文本
/// 支持: .txt, .md, .csv, .json, .yaml, .pdf, .docx, .pptx, .xlsx
/// 图片/音视频: 仅提取文件名作为语义占位符

// ═══════════════════════════════════════════════════════════
// 数据结构
// ═══════════════════════════════════════════════════════════

/// 单个文件的提取结果
#[derive(Debug, Clone)]
pub struct ExtractedFile {
    pub relative_path: String,
    pub file_name: String,
    pub file_type: String,      // "text", "pdf", "docx", "pptx", "xlsx", "image", "other"
    pub text_content: String,
    pub char_count: usize,
    pub byte_size: u64,
}

/// 文件夹预估结果
#[derive(Debug, Clone)]
pub struct EstimateResult {
    pub convertable_files: usize,
    pub convertable_bytes: u64,
    pub skipped_files: usize,
    pub estimated_tokens: usize,
    pub estimated_cost_cheap_rmb: f64,
    pub estimated_cost_core_rmb: f64,
}

// ═══════════════════════════════════════════════════════════
// 后缀名分类
// ═══════════════════════════════════════════════════════════

/// 判断文件后缀是否是我们能处理的文本类
fn is_text_ext(ext: &str) -> bool {
    matches!(ext, "txt" | "md" | "csv" | "json" | "yaml" | "yml" | "toml" | "xml" | "html" | "htm" | "log" | "ini" | "cfg" | "conf" | "rst" | "tex")
}

fn is_pdf(ext: &str) -> bool { ext == "pdf" }
fn is_docx(ext: &str) -> bool { ext == "docx" }
fn is_pptx(ext: &str) -> bool { ext == "pptx" }
fn is_xlsx(ext: &str) -> bool { ext == "xlsx" || ext == "xls" }

fn is_image(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" | "tif")
}

fn is_media(ext: &str) -> bool {
    matches!(ext, "mp3" | "mp4" | "wav" | "avi" | "mkv" | "mov" | "flac" | "ogg" | "m4a" | "wmv")
}

/// 跳过不需要扫描的目录
fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist" | "build" | "__pycache__" | ".git" | "venv" | ".venv")
}

// ═══════════════════════════════════════════════════════════
// 核心提取逻辑
// ═══════════════════════════════════════════════════════════

/// 提取纯文本文件
fn extract_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("读取文本失败: {}", e))
}

/// 提取 PDF 文本 (使用 pdf-extract crate)
fn extract_pdf(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("读取 PDF 文件失败: {}", e))?;

    pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| format!("PDF 解析失败: {}", e))
}

/// 提取 DOCX 文本 (ZIP 解压 → XML 解析 → <w:t> 节点)
fn extract_docx(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path)
        .map_err(|e| format!("打开 DOCX 失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("DOCX ZIP 解压失败: {}", e))?;

    // DOCX 的正文在 word/document.xml 中
    let mut doc_xml = match archive.by_name("word/document.xml") {
        Ok(f) => f,
        Err(_) => return Err("DOCX 中找不到 word/document.xml".to_string()),
    };

    let mut xml_content = String::new();
    std::io::Read::read_to_string(&mut doc_xml, &mut xml_content)
        .map_err(|e| format!("读取 DOCX XML 失败: {}", e))?;

    // 使用 quick-xml 提取 <w:t> 标签中的文本
    extract_xml_text_nodes(&xml_content, "w:t")
}

/// 提取 PPTX 文本 (ZIP 解压 → 遍历 slides → <a:t> 节点)
fn extract_pptx(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path)
        .map_err(|e| format!("打开 PPTX 失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("PPTX ZIP 解压失败: {}", e))?;

    let mut all_text = String::new();

    // 遍历所有 slide 文件 (ppt/slides/slide1.xml, slide2.xml, ...)
    let slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive.by_index(i).ok().and_then(|f| {
                let name = f.name().to_string();
                if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                    Some(name)
                } else {
                    None
                }
            })
        })
        .collect();

    for slide_name in slide_names {
        if let Ok(mut slide_file) = archive.by_name(&slide_name) {
            let mut xml_content = String::new();
            if std::io::Read::read_to_string(&mut slide_file, &mut xml_content).is_ok() {
                if let Ok(text) = extract_xml_text_nodes(&xml_content, "a:t") {
                    if !text.is_empty() {
                        all_text.push_str(&format!("\n--- Slide ---\n{}\n", text));
                    }
                }
            }
        }
    }

    if all_text.is_empty() {
        Err("PPTX 中未找到文本内容".to_string())
    } else {
        Ok(all_text)
    }
}

/// 提取 XLSX 文本 (使用 calamine crate)
fn extract_xlsx(path: &Path) -> Result<String, String> {
    use calamine::{Reader, open_workbook_auto};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| format!("打开 Excel 失败: {}", e))?;

    let mut all_text = String::new();
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

    for sheet_name in sheet_names {
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            all_text.push_str(&format!("\n## Sheet: {}\n", sheet_name));
            for row in range.rows() {
                let cells: Vec<String> = row.iter().map(|cell| {
                    format!("{}", cell)
                }).collect();
                all_text.push_str(&cells.join("\t"));
                all_text.push('\n');
            }
        }
    }

    if all_text.is_empty() {
        Err("Excel 中未找到数据".to_string())
    } else {
        Ok(all_text)
    }
}

/// 通用 XML 文本节点提取器
/// tag_name: 例如 "w:t" (Word) 或 "a:t" (PowerPoint)
fn extract_xml_text_nodes(xml: &str, tag_name: &str) -> Result<String, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut text_parts: Vec<String> = Vec::new();
    let mut inside_target = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local_name = e.local_name();
                let name_str = std::str::from_utf8(local_name.as_ref()).unwrap_or("");
                // 匹配 "t" (local name of w:t or a:t)
                if name_str == "t" || std::str::from_utf8(e.name().as_ref()).unwrap_or("") == tag_name {
                    inside_target = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if inside_target {
                    if let Ok(text) = e.unescape() {
                        text_parts.push(text.into_owned());
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = e.local_name();
                let name_str = std::str::from_utf8(local_name.as_ref()).unwrap_or("");
                if name_str == "t" || std::str::from_utf8(e.name().as_ref()).unwrap_or("") == tag_name {
                    inside_target = false;
                }
                // 段落结束添加换行
                let end_local = e.local_name();
                let end_name_str = std::str::from_utf8(end_local.as_ref()).unwrap_or("");
                if end_name_str == "p" {
                    text_parts.push("\n".to_string());
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML 解析错误: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(text_parts.join(""))
}

// ═══════════════════════════════════════════════════════════
// 公开接口
// ═══════════════════════════════════════════════════════════

/// 提取整个文件夹中所有可转换文件的纯文本
pub fn extract_folder(folder_path: &str) -> Vec<ExtractedFile> {
    let root = Path::new(folder_path);
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for entry in WalkDir::new(folder_path)
        .max_depth(20)
        .into_iter()
        .filter_entry(|e| {
            !e.file_name().to_str().map_or(false, should_skip_dir) || e.path() == root
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() { continue; }

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let relative_path = path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let byte_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        // 跳过超大文件 (> 50MB)
        if byte_size > 50 * 1024 * 1024 {
            continue;
        }

        let (file_type, text_result) = if is_text_ext(&ext) {
            ("text".to_string(), extract_text(path))
        } else if is_pdf(&ext) {
            ("pdf".to_string(), extract_pdf(path))
        } else if is_docx(&ext) {
            ("docx".to_string(), extract_docx(path))
        } else if is_pptx(&ext) {
            ("pptx".to_string(), extract_pptx(path))
        } else if is_xlsx(&ext) {
            ("xlsx".to_string(), extract_xlsx(path))
        } else if is_image(&ext) {
            ("image".to_string(), Ok(format!("[图片: {}]", file_name)))
        } else if is_media(&ext) {
            ("media".to_string(), Ok(format!("[媒体文件: {}]", file_name)))
        } else {
            continue; // 跳过未知格式
        };

        let text_content = match text_result {
            Ok(text) => text,
            Err(e) => {
                log::warn!("提取文件失败 {}: {}", relative_path, e);
                format!("[提取失败: {}]", e)
            }
        };

        let char_count = text_content.chars().count();

        results.push(ExtractedFile {
            relative_path,
            file_name,
            file_type,
            text_content,
            char_count,
            byte_size,
        });
    }

    results
}

/// 快速预估文件夹的可转换文件数量和费用（不提取文本，只统计）
pub fn estimate_folder(folder_path: &str) -> EstimateResult {
    let root = Path::new(folder_path);
    if !root.exists() || !root.is_dir() {
        return EstimateResult {
            convertable_files: 0,
            convertable_bytes: 0,
            skipped_files: 0,
            estimated_tokens: 0,
            estimated_cost_cheap_rmb: 0.0,
            estimated_cost_core_rmb: 0.0,
        };
    }

    let mut convertable_files = 0usize;
    let mut convertable_bytes = 0u64;
    let mut skipped_files = 0usize;

    for entry in WalkDir::new(folder_path)
        .max_depth(20)
        .into_iter()
        .filter_entry(|e| {
            !e.file_name().to_str().map_or(false, should_skip_dir) || e.path() == root
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() { continue; }

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let byte_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        if byte_size > 50 * 1024 * 1024 {
            skipped_files += 1;
            continue;
        }

        if is_text_ext(&ext) || is_pdf(&ext) || is_docx(&ext) || is_pptx(&ext) || is_xlsx(&ext) || is_image(&ext) {
            convertable_files += 1;
            convertable_bytes += byte_size;
        } else {
            skipped_files += 1;
        }
    }

    // 粗略估算 Token 数: 中文约 2 字符/token，英文约 4 字符/token
    // 保守取 3 字符/token 的平均值
    let estimated_chars = convertable_bytes as f64 * 0.8; // 假设 80% 是有效文本
    let estimated_tokens = (estimated_chars / 3.0) as usize;

    // 费用估算 (基于 RMB/百万Token)
    // Cheap (如 GLM-4-Flash): 输入 0.1 元/百万Token
    // Core (如 DeepSeek V4 Flash): 输入 1.0 元/百万Token
    let cost_cheap = (estimated_tokens as f64 / 1_000_000.0) * 0.1;
    let cost_core = (estimated_tokens as f64 / 1_000_000.0) * 1.0;

    EstimateResult {
        convertable_files,
        convertable_bytes,
        skipped_files,
        estimated_tokens,
        estimated_cost_cheap_rmb: cost_cheap,
        estimated_cost_core_rmb: cost_core,
    }
}

// ═══════════════════════════════════════════════════════════
// Tauri Command 暴露
// ═══════════════════════════════════════════════════════════

/// 预估知识库构建费用 — 替换 tauri-bridge.js 中的 Mock
#[tauri::command]
pub fn system_estimate_kb(folder_path: String) -> Value {
    let result = estimate_folder(&folder_path);
    json!({
        "convertable_files": result.convertable_files,
        "convertable_bytes": result.convertable_bytes,
        "skipped_files": result.skipped_files,
        "estimated_tokens": result.estimated_tokens,
        "estimated_cost_cheap_rmb": result.estimated_cost_cheap_rmb,
        "estimated_cost_core_rmb": result.estimated_cost_core_rmb,
    })
}

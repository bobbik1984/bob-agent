use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// 扫描外部技能目录，解析每个子文件夹中的 SKILL.md 的 YAML frontmatter
/// 同时注册内置的系统级工具
#[tauri::command]
pub fn system_get_plugins() -> Value {
    let mut plugins: Vec<Value> = Vec::new();

    // ── 1. 内置系统能力 (Native Tools) ──────────────────────
    plugins.push(json!({
        "id": "native-llm-chat",
        "name": "LLM 对话引擎",
        "type": "tool",
        "typeLabel": "内置",
        "description": "基于 Rust reqwest 的多供应商大模型流式对话引擎 (SSE)",
        "installed": true
    }));
    plugins.push(json!({
        "id": "native-vision",
        "name": "图像视觉分析",
        "type": "tool",
        "typeLabel": "内置",
        "description": "支持 Base64 图片上传 + 多模态视觉问答",
        "installed": true
    }));
    plugins.push(json!({
        "id": "native-filesystem",
        "name": "文件系统扫描",
        "type": "tool",
        "typeLabel": "内置",
        "description": "递归文件夹扫描 (walkdir)，支持智能剪枝和 50K 熔断保护",
        "installed": true
    }));
    plugins.push(json!({
        "id": "native-sqlite",
        "name": "SQLite 对话存储",
        "type": "tool",
        "typeLabel": "内置",
        "description": "基于 rusqlite 的本地对话/消息持久化引擎 (WAL 模式)",
        "installed": true
    }));
    plugins.push(json!({
        "id": "native-credential",
        "name": "凭证安全存储",
        "type": "tool",
        "typeLabel": "内置",
        "description": "API Key 加密存储与多供应商凭证管理",
        "installed": true
    }));

    // ── 2. 外部认知技能 (External Skills) ──────────────────
    let config = super::read_config();
    if let Some(skills_dir) = config.get("externalSkillsDir").and_then(|v| v.as_str()) {
        let dir_path = Path::new(skills_dir);
        if dir_path.exists() && dir_path.is_dir() {
            if let Ok(entries) = fs::read_dir(dir_path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if !entry_path.is_dir() {
                        continue;
                    }

                    let skill_md = entry_path.join("SKILL.md");
                    if !skill_md.exists() {
                        continue;
                    }

                    // 读取 SKILL.md 并解析 YAML frontmatter
                    let folder_name = entry_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let (name, description) = match fs::read_to_string(&skill_md) {
                        Ok(content) => parse_skill_frontmatter(&content, &folder_name),
                        Err(_) => (folder_name.clone(), String::new()),
                    };

                    plugins.push(json!({
                        "id": format!("skill-{}", folder_name),
                        "name": name,
                        "type": "skill",
                        "typeLabel": "外部技能",
                        "description": description,
                        "installed": true,
                        "path": entry_path.to_string_lossy()
                    }));
                }
            }
        }
    }

    json!(plugins)
}

/// 解析 SKILL.md 中的 YAML frontmatter (--- ... --- 块)
/// 提取 name 和 description 字段
fn parse_skill_frontmatter(content: &str, fallback_name: &str) -> (String, String) {
    let mut name = fallback_name.to_string();
    let mut description = String::new();

    // 检查是否以 --- 开头（YAML frontmatter 标记）
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        // 没有 frontmatter，尝试从第一行提取标题
        if let Some(first_line) = trimmed.lines().next() {
            let clean = first_line.trim_start_matches('#').trim();
            if !clean.is_empty() {
                name = clean.to_string();
            }
        }
        return (name, description);
    }

    // 找到第二个 --- 的位置
    let after_first = &trimmed[3..];
    if let Some(end_pos) = after_first.find("\n---") {
        let yaml_block = &after_first[..end_pos];

        for line in yaml_block.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("name:") {
                name = val.trim().trim_matches('"').trim_matches('\'').to_string();
            } else if let Some(val) = line.strip_prefix("description:") {
                description = val.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }

    (name, description)
}

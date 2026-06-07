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

    let config = super::read_config();
    let bundled_dir = config.get("bundledSkillsDir").and_then(|v| v.as_str()).map(|s| Path::new(s).to_path_buf());
    let external_dir = config.get("externalSkillsDir").and_then(|v| v.as_str()).map(|s| Path::new(s).to_path_buf());
    
    let mut added_external = std::collections::HashSet::new();

    // ── 2. 外部自定义技能 (External Skills) ──────────────────
    if let Some(dir_path) = &external_dir {
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

                    let folder_name = entry_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let (name, description) = match fs::read_to_string(&skill_md) {
                        Ok(content) => crate::tools::parse_skill_frontmatter(&content, &folder_name),
                        Err(_) => (folder_name.clone(), String::new()),
                    };

                    let is_overriding = bundled_dir.as_ref().map(|b| b.join(&folder_name).exists()).unwrap_or(false);

                    plugins.push(json!({
                        "id": format!("skill-external-{}", folder_name),
                        "name": name,
                        "type": "skill",
                        "typeLabel": "外部技能",
                        "is_official": false,
                        "is_overriding": is_overriding,
                        "description": description,
                        "installed": true,
                        "path": entry_path.to_string_lossy()
                    }));
                    added_external.insert(folder_name);
                }
            }
        }
    }

    // ── 3. 内置官方技能 (Bundled Skills) ──────────────────
    if let Some(dir_path) = &bundled_dir {
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

                    let folder_name = entry_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let (name, description) = match fs::read_to_string(&skill_md) {
                        Ok(content) => crate::tools::parse_skill_frontmatter(&content, &folder_name),
                        Err(_) => (folder_name.clone(), String::new()),
                    };

                    let is_overridden = added_external.contains(&folder_name);

                    plugins.push(json!({
                        "id": format!("skill-official-{}", folder_name),
                        "name": name,
                        "type": "skill",
                        "typeLabel": "官方技能",
                        "is_official": true,
                        "is_overridden": is_overridden,
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

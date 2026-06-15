use serde_json::{json, Value};
use std::fs;
use tauri::{AppHandle, Emitter, Manager};
use crate::db::DbState;

/// LLM-Wiki 知识库引擎 — Phase B: 异步 Ingest 引擎
///
/// 核心设计: 使用 Tokio::spawn 后台线程运行 Clerk 模型摘要，
/// 完全不阻塞主聊天流，用户可以同时和主模型对话。
/// 进度通过 Tauri Event (kb:progress / kb:complete) 推送到前端。

// ═══════════════════════════════════════════════════════════
// Ingest Prompt 模板
// ═══════════════════════════════════════════════════════════

const INGEST_PROMPT: &str = r#"你是一个文件摘要助手。请严格按以下 JSON 格式输出，不要有任何多余文字：

{
  "summary": "不超过 300 字的核心内容摘要",
  "keywords": ["关键词1", "关键词2", "关键词3", "关键词4", "关键词5"],
  "entities": [
    {"name": "实体名称", "type": "人物|组织|概念|地点|政策|技术|项目", "description": "一句话描述"}
  ],
  "relations": [
    {"source": "实体A名称", "target": "实体B名称", "relation": "uses|depends_on|contains|related_to|implements|created_by", "confidence": 0.9}
  ],
  "data_points": [
    "关键数据点1: 具体数值或事实",
    "关键数据点2: 具体数值或事实"
  ]
}

注意：relations 中的 source 和 target 必须是 entities 中已定义的实体名称。
以下是文件的原始文本（可能有 OCR 乱码，请忽略排版错误）：
"#;

// ═══════════════════════════════════════════════════════════
// Wiki 目录管理（路径统一由 lib.rs::get_wiki_dir() 提供）
// ═══════════════════════════════════════════════════════════

fn ensure_wiki_structure() {
    let wiki = super::get_wiki_dir();
    let _ = fs::create_dir_all(wiki.join("sources"));
    let _ = fs::create_dir_all(wiki.join("entities"));
    let _ = fs::create_dir_all(wiki.join("projects"));

    // 确保 index.md 存在
    let index_path = wiki.join("index.md");
    if !index_path.exists() {
        let _ = fs::write(&index_path,
            "# 知识库索引\n\n> 此文件由 Bob-Agent LLM-Wiki 引擎自动维护。\n\n## Sources (文件摘要)\n\n## Entities (实体/概念)\n\n## Projects (项目综述)\n\n"
        );
    }

    // 确保 log.md 存在
    let log_path = wiki.join("log.md");
    if !log_path.exists() {
        let _ = fs::write(&log_path,
            "# 知识库操作日志\n\n> Append-only 时间轴，记录每次 Ingest 操作。\n\n"
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Wiki 写入工具
// ═══════════════════════════════════════════════════════════

/// 写入一个 source 摘要页，同时同步到 FTS5 搜索索引
fn write_source_page(file_name: &str, file_type: &str, summary: &str, keywords: &[String], data_points: &[String]) {
    let wiki = super::get_wiki_dir();
    // 清理文件名中的特殊字符
    let safe_name: String = file_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let page_name = if safe_name.contains('.') {
        safe_name.rsplit_once('.').map(|(n, _)| n).unwrap_or(&safe_name).to_string()
    } else {
        safe_name.clone()
    };

    let page_path = wiki.join("sources").join(format!("{}.md", page_name));

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let tags = keywords.join(", ");

    let content = format!(
        "---\nsource: {}\ntype: {}\ntags: [{}]\nindexed_at: {}\n---\n\n# {}\n\n## 摘要\n\n{}\n\n## 关键数据点\n\n{}\n",
        file_name, file_type, tags, now, page_name,
        summary,
        data_points.iter().map(|d| format!("- {}", d)).collect::<Vec<_>>().join("\n")
    );

    let _ = fs::write(&page_path, content);

    // 同步写入 FTS5 搜索索引
    let wiki_rel_path = format!("wiki/sources/{}.md", page_name);
    if let Ok(db) = rusqlite::Connection::open(super::get_data_dir().join("bob.db")) {
        let _ = db.execute(
            "INSERT INTO wiki_fts (file_name, source_path, wiki_path, summary, keywords, category, indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                file_name,
                file_name,     // source_path: 后续由调用方传入绝对路径
                wiki_rel_path,
                summary,
                tags,
                file_type,
                now
            ],
        );
    }
}

/// 追加 index.md 中的条目
fn append_index_entry(file_name: &str, summary_oneliner: &str) {
    let wiki = super::get_wiki_dir();
    let index_path = wiki.join("index.md");

    // 清理文件名
    let safe_name: String = file_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let page_name = if safe_name.contains('.') {
        safe_name.rsplit_once('.').map(|(n, _)| n).unwrap_or(&safe_name).to_string()
    } else {
        safe_name.clone()
    };

    let entry = format!("- [{}](sources/{}.md) — {}\n", page_name, page_name, summary_oneliner);

    if let Ok(mut content) = fs::read_to_string(&index_path) {
        // 在 "## Sources" 段落之后插入
        if let Some(pos) = content.find("## Sources") {
            if let Some(newline_pos) = content[pos..].find('\n') {
                let insert_at = pos + newline_pos + 1;
                // 跳过紧跟的空行
                let actual_insert = if content[insert_at..].starts_with('\n') {
                    insert_at + 1
                } else {
                    insert_at
                };
                content.insert_str(actual_insert, &entry);
                let _ = fs::write(&index_path, content);
                return;
            }
        }
        // Fallback: 直接追加到文件末尾
        let _ = fs::write(&index_path, format!("{}\n{}", content, entry));
    }
}

/// 追加 log.md 记录
fn append_log_entry(action: &str, detail: &str) {
    let wiki = super::get_wiki_dir();
    let log_path = wiki.join("log.md");
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let entry = format!("## [{}] {} | {}\n\n", now, action, detail);

    if let Ok(content) = fs::read_to_string(&log_path) {
        let _ = fs::write(&log_path, format!("{}{}", content, entry));
    }
}

/// 写入项目综述页
fn write_project_page(folder_name: &str, file_count: usize, summaries: &[(String, String)]) {
    let wiki = super::get_wiki_dir();
    let safe_name: String = folder_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let page_path = wiki.join("projects").join(format!("{}.md", safe_name));

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut content = format!(
        "# {}\n\n> 此页面由 LLM-Wiki 引擎自动生成于 {}，包含 {} 个文件的综述。\n\n## 文件清单\n\n",
        folder_name, now, file_count
    );

    for (name, oneliner) in summaries {
        let page_name = if name.contains('.') {
            name.rsplit_once('.').map(|(n, _)| n).unwrap_or(name).to_string()
        } else {
            name.clone()
        };
        content.push_str(&format!("- [{}](../sources/{}.md) — {}\n", name, page_name, oneliner));
    }

    let _ = fs::write(&page_path, content);

    // 同步更新 index.md 的 Projects 段落
    let index_path = wiki.join("index.md");
    if let Ok(mut index_content) = fs::read_to_string(&index_path) {
        let entry = format!("- [{}](projects/{}.md) — {} 个文件的综述\n", folder_name, safe_name, file_count);
        if let Some(pos) = index_content.find("## Projects") {
            if let Some(newline_pos) = index_content[pos..].find('\n') {
                let insert_at = pos + newline_pos + 1;
                let actual_insert = if index_content[insert_at..].starts_with('\n') {
                    insert_at + 1
                } else {
                    insert_at
                };
                index_content.insert_str(actual_insert, &entry);
                let _ = fs::write(&index_path, index_content);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Clerk 模型调用 (复用 llm.rs 的 HTTP 逻辑)
// ═══════════════════════════════════════════════════════════

/// 对文本片段调用 Clerk 模型获取结构化摘要
async fn call_clerk_for_summary(file_name: &str, text_chunk: &str) -> Result<Value, String> {
    let config = super::read_config();

    // 1. 获取 Clerk 模型配置
    let clerk_model = config.get("clerkModel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if clerk_model.is_empty() {
        return Err("Clerk 模型未配置".to_string());
    }

    // 2. 反查 provider 和 API Key
    let pool = super::llm::get_model_pool();
    let mut provider = String::new();
    if let Some(arr) = pool.as_array() {
        if let Some(model_info) = arr.iter().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(&clerk_model)) {
            provider = model_info.get("provider").and_then(|v| v.as_str()).unwrap_or("").to_string();
        }
    }

    if provider.is_empty() {
        return Err(format!("无法找到 Clerk 模型 {} 的供应商信息", clerk_model));
    }

    let api_keys = super::llm::get_api_keys();
    let api_key = api_keys.get(&provider)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if api_key.is_empty() && provider != "offline" {
        return Err(format!("供应商 {} 的 API Key 未配置", provider));
    }

    // 3. 构建请求 URL
    let base_url = match provider.as_str() {
        "minimax" => "https://api.minimax.chat/v1",
        "deepseek" => "https://api.deepseek.com",
        "openai" => "https://api.openai.com/v1",
        "qwen" => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "doubao" => "https://ark.cn-beijing.volces.com/api/v3",
        "zhipu" => "https://open.bigmodel.cn/api/paas/v4",
        "kimi" => "https://api.moonshot.cn/v1",
        "offline" => "http://127.0.0.1:11434/v1",
        _ => "https://api.openai.com/v1",
    };

    let url = format!("{}/chat/completions", base_url);

    // 4. 截取文本块（防止超出上下文窗口）
    let truncated: String = text_chunk.chars().take(8000).collect();

    let user_content = format!(
        "{}\n\n文件名: {}\n\n{}",
        INGEST_PROMPT, file_name, truncated
    );

    let body = json!({
        "model": clerk_model,
        "messages": [
            { "role": "user", "content": user_content }
        ],
        "temperature": 0.3,
        "max_tokens": 1000
    });

    // 5. 发送请求
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {}", e))?;

    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Clerk 请求失败: {}", e))?;

    let resp_json: Value = resp.json().await
        .map_err(|e| format!("Clerk 响应解析失败: {}", e))?;

    // 6. 提取回复内容
    let content = resp_json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    // 7. 尝试解析为 JSON
    // 先尝试提取 ``` 代码块中的 JSON
    let json_str = if content.contains("```json") {
        content.split("```json").nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(&content)
    } else if content.contains("```") {
        content.split("```").nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(&content)
    } else {
        &content
    };

    serde_json::from_str(json_str.trim())
        .map_err(|_| {
            // 如果 JSON 解析失败，回退为简单摘要
            log::warn!("Clerk 返回了非 JSON 格式，回退为纯文本摘要");
            format!("FALLBACK:{}", content)
        })
}

// ═══════════════════════════════════════════════════════════
// 主入口: 异步后台 KB 构建
// ═══════════════════════════════════════════════════════════

/// 后台异步构建知识库 — 通过 Tokio::spawn 运行，不阻塞主聊天
#[tauri::command]
pub async fn system_build_kb(app: AppHandle, folder_path: String, _plan: String) -> Value {
    // 0. 检查 Clerk 模型配置
    let config = super::read_config();
    let clerk_model = config.get("clerkModel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if clerk_model.is_empty() {
        return json!({
            "error": true,
            "message": "请先在设置中配置「牛马模型 (Clerk)」，用于知识库构建。"
        });
    }

    // 1. 确保 Wiki 目录结构
    ensure_wiki_structure();

    // 2. 提取所有文件
    let _ = app.emit("kb:progress", json!({
        "message": "正在扫描文件/文件夹...",
        "current": 0,
        "total": 0,
        "phase": "extract"
    }));

    let files = super::kb_extractor::extract_folder(&folder_path);
    let total = files.len();

    if total == 0 {
        return json!({
            "error": true,
            "message": "路径中没有找到可处理的文件。"
        });
    }

    // 3. 获取文件夹名
    let folder_name = std::path::Path::new(&folder_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    // 4. 逐文件处理
    let mut success_count = 0usize;
    let mut failed_count = 0usize;
    let mut summaries: Vec<(String, String)> = Vec::new();

    for (i, file) in files.iter().enumerate() {
        // 跳过图片/媒体的 LLM 调用（直接记录文件名）
        if file.file_type == "image" || file.file_type == "media" {
            let oneliner = format!("[{}] {}", file.file_type, file.file_name);
            append_index_entry(&file.file_name, &oneliner);
            summaries.push((file.file_name.clone(), oneliner));
            success_count += 1;

            let _ = app.emit("kb:progress", json!({
                "message": format!("已记录 {} ({})", file.file_name, file.file_type),
                "current": i + 1,
                "total": total,
                "phase": "index"
            }));
            continue;
        }

        // 发送进度
        let _ = app.emit("kb:progress", json!({
            "message": format!("正在阅读 {}", file.file_name),
            "current": i + 1,
            "total": total,
            "phase": "ingest"
        }));

        // 调用 Clerk 模型
        match call_clerk_for_summary(&file.file_name, &file.text_content).await {
            Ok(result) => {
                let summary = result.get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("（无摘要）")
                    .to_string();

                let keywords: Vec<String> = result.get("keywords")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();

                let data_points: Vec<String> = result.get("data_points")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();

                // M17: 收集实体信息用于图谱写入
                let mut entity_names_for_kg: Vec<(String, String, String)> = Vec::new();

                // 写入 Wiki
                write_source_page(&file.file_name, &file.file_type, &summary, &keywords, &data_points);

                // 生成一行摘要用于 index
                let oneliner: String = summary.chars().take(80).collect();
                append_index_entry(&file.file_name, &oneliner);
                summaries.push((file.file_name.clone(), oneliner.clone()));

                // 处理实体
                if let Some(entities) = result.get("entities").and_then(|v| v.as_array()) {
                    for entity in entities {
                        if let (Some(name), Some(etype), Some(desc)) = (
                            entity.get("name").and_then(|v| v.as_str()),
                            entity.get("type").and_then(|v| v.as_str()),
                            entity.get("description").and_then(|v| v.as_str()),
                        ) {
                            // TODO: 未来可以合并同名实体页
                            let entity_path = super::get_wiki_dir().join("entities").join(format!("{}.md", name));
                            if !entity_path.exists() {
                                let entity_content = format!(
                                    "# {}\n\n**类型**: {}\n\n{}\n\n## 相关文件\n\n- [{}](../sources/{}.md)\n",
                                    name, etype, desc, file.file_name,
                                    file.file_name.rsplit_once('.').map(|(n, _)| n).unwrap_or(&file.file_name)
                                );
                                let _ = fs::write(&entity_path, entity_content);
                            }

                            // M17: 写入知识图谱节点
                            entity_names_for_kg.push((name.to_string(), etype.to_string(), desc.to_string()));
                        }
                    }
                }

                // M17: 写入知识图谱 (kg_nodes + kg_edges)
                if let Some(db_state) = app.try_state::<DbState>() {
                    if let Ok(conn) = db_state.0.lock() {
                        // 写入实体节点
                        for (name, etype, desc) in &entity_names_for_kg {
                            let node_id = format!("{}_{}", etype, name.to_lowercase().replace(' ', "_"));
                            let _ = crate::kg::upsert_node(&conn, &node_id, name, etype, desc, &file.file_name);
                        }
                        // 写入文件节点（文件本身也是一个节点）
                        let file_node_id = format!("file_{}", file.file_name.to_lowercase().replace(' ', "_").replace('.', "_"));
                        let _ = crate::kg::upsert_node(&conn, &file_node_id, &file.file_name, "file", &summary, &file.file_name);
                        // 文件 -> 实体 边 (contains)
                        for (name, etype, _) in &entity_names_for_kg {
                            let node_id = format!("{}_{}", etype, name.to_lowercase().replace(' ', "_"));
                            let _ = crate::kg::insert_edge(&conn, &file_node_id, &node_id, "contains", 0.9);
                        }
                        // 写入 LLM 提取的 relations
                        if let Some(relations) = result.get("relations").and_then(|v| v.as_array()) {
                            for rel in relations {
                                if let (Some(src_name), Some(tgt_name), Some(relation)) = (
                                    rel.get("source").and_then(|v| v.as_str()),
                                    rel.get("target").and_then(|v| v.as_str()),
                                    rel.get("relation").and_then(|v| v.as_str()),
                                ) {
                                    let confidence = rel.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.8);
                                    // 尝试匹配已有实体 ID
                                    let src_id = entity_names_for_kg.iter()
                                        .find(|(n, _, _)| n == src_name)
                                        .map(|(n, t, _)| format!("{}_{}", t, n.to_lowercase().replace(' ', "_")))
                                        .unwrap_or_else(|| format!("concept_{}", src_name.to_lowercase().replace(' ', "_")));
                                    let tgt_id = entity_names_for_kg.iter()
                                        .find(|(n, _, _)| n == tgt_name)
                                        .map(|(n, t, _)| format!("{}_{}", t, n.to_lowercase().replace(' ', "_")))
                                        .unwrap_or_else(|| format!("concept_{}", tgt_name.to_lowercase().replace(' ', "_")));
                                    let _ = crate::kg::insert_edge(&conn, &src_id, &tgt_id, relation, confidence);
                                }
                            }
                        }
                    }
                }

                success_count += 1;
                append_log_entry("ingest", &format!("{} → ✅ {}", file.file_name, oneliner));
            }
            Err(e) => {
                if e.starts_with("FALLBACK:") {
                    // 模型返回了非 JSON，但有文本，用作简单摘要
                    let fallback_text = &e[9..];
                    let oneliner: String = fallback_text.chars().take(80).collect();
                    write_source_page(&file.file_name, &file.file_type, fallback_text, &[], &[]);
                    append_index_entry(&file.file_name, &oneliner);
                    summaries.push((file.file_name.clone(), oneliner));
                    success_count += 1;
                    append_log_entry("ingest", &format!("{} → ⚠️ fallback", file.file_name));
                } else {
                    failed_count += 1;
                    append_log_entry("ingest", &format!("{} → ❌ {}", file.file_name, e));
                    log::error!("KB Ingest 失败 {}: {}", file.file_name, e);
                }
            }
        }
    }

    // 5. 生成项目综述页
    write_project_page(&folder_name, total, &summaries);
    append_log_entry("complete", &format!("项目 {} 构建完成: {}/{} 成功", folder_name, success_count, total));

    // 6. 发送完成事件
    let _ = app.emit("kb:complete", json!({
        "folder": folder_name,
        "total": total,
        "success": success_count,
        "failed": failed_count
    }));

    json!({
        "ok": true,
        "folder": folder_name,
        "total": total,
        "success": success_count,
        "failed": failed_count
    })
}

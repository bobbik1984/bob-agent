use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};


// ── 供应商默认模型 ─────────────────────────────────────
fn get_default_model(provider: &str) -> &str {
    match provider {
        "deepseek" => "deepseek-v4-flash",
        "openai" => "gpt-4.1-mini",
        "qwen" => "qwen3.6-plus",
        "doubao" => "doubao-seed-1-6-flash-250828",
        "zhipu" => "GLM-4.7",
        "kimi" => "kimi-k2.5",
        "minimax" => "MiniMax-M2.7",
        _ => "",
    }
}

// ── ModelHub API ─────────────────────────────────────────

pub fn get_models(provider_opt: Option<String>) -> Value {
    let provider = provider_opt.unwrap_or_else(|| {
        super::read_config().get("provider").and_then(|v| v.as_str()).unwrap_or("deepseek").to_string()
    });
    let pool = get_model_pool();
    if let Some(arr) = pool.as_array() {
        let filtered: Vec<Value> = arr.iter().filter(|m| {
            m.get("provider").and_then(|v| v.as_str()) == Some(provider.as_str())
        }).cloned().collect();
        return json!(filtered);
    }
    json!([])
}

pub fn get_model_pool() -> Value {
    let static_pool = json!([
        { "id": "deepseek-v4-flash", "modelId": "deepseek-v4-flash", "displayName": "DeepSeek V4 Flash", "label": "DeepSeek V4 Flash", "provider": "deepseek", "providerName": "DeepSeek", "vision": true, "default": true, "pricing": { "input": 1.0, "output": 2.0 } },
        { "id": "deepseek-v4-pro", "modelId": "deepseek-v4-pro", "displayName": "DeepSeek V4 Pro", "label": "DeepSeek V4 Pro", "provider": "deepseek", "providerName": "DeepSeek", "vision": false, "pricing": { "input": 3.0, "output": 6.0 } },
        
        { "id": "gpt-4.1", "modelId": "gpt-4.1", "displayName": "GPT-4.1", "label": "GPT-4.1", "provider": "openai", "providerName": "OpenAI", "vision": true, "pricing": { "input": 35.0, "output": 105.0 } },
        { "id": "gpt-4.1-mini", "modelId": "gpt-4.1-mini", "displayName": "GPT-4.1 Mini", "label": "GPT-4.1 Mini", "provider": "openai", "providerName": "OpenAI", "vision": true, "default": true, "pricing": { "input": 1.05, "output": 4.2 } },
        
        { "id": "qwen3.6-plus", "modelId": "qwen3.6-plus", "displayName": "Qwen3.6 Plus", "label": "Qwen3.6 Plus", "provider": "qwen", "providerName": "阿里云百炼", "vision": true, "default": true, "pricing": { "input": 2.0, "output": 12.0 } },
        { "id": "qwen3.6-max-preview", "modelId": "qwen3.6-max-preview", "displayName": "Qwen3.6 Max", "label": "Qwen3.6 Max", "provider": "qwen", "providerName": "阿里云百炼", "vision": true, "pricing": { "input": 2.4, "output": 9.6 } },
        
        { "id": "doubao-seed-1-6-flash-250828", "modelId": "doubao-seed-1-6-flash-250828", "displayName": "Doubao Seed 1.6 Flash", "label": "Doubao Seed 1.6 Flash", "provider": "doubao", "providerName": "字节火山", "vision": false, "default": true, "pricing": { "input": 0.8, "output": 2.0 } },
        { "id": "doubao-seed-2-0-mini-260215", "modelId": "doubao-seed-2-0-mini-260215", "displayName": "Doubao Seed 2.0 Mini", "label": "Doubao Seed 2.0 Mini", "provider": "doubao", "providerName": "字节火山", "vision": true, "pricing": { "input": 0.8, "output": 2.0 } },
        
        { "id": "local-qwen2.5-1.5b", "modelId": "qwen2.5-1.5b", "displayName": "Qwen 2.5 1.5B (离线)", "label": "Qwen 2.5 1.5B (离线)", "provider": "offline", "providerName": "本地离线计算", "vision": false, "default": true, "pricing": { "input": 0.0, "output": 0.0 } },
        { "id": "local-llama-3.2-1b", "modelId": "llama-3.2-1b", "displayName": "Llama 3.2 1B (离线)", "label": "Llama 3.2 1B (离线)", "provider": "offline", "providerName": "本地离线计算", "vision": false, "pricing": { "input": 0.0, "output": 0.0 } },

        { "id": "GLM-4.7", "modelId": "GLM-4.7", "displayName": "GLM-4.7", "label": "GLM-4.7", "provider": "zhipu", "providerName": "智谱 (GLM)", "vision": false, "default": true, "pricing": { "input": 2.0, "output": 8.0 } },
        
        { "id": "kimi-k2.5", "modelId": "kimi-k2.5", "displayName": "Kimi k2.5", "label": "Kimi k2.5", "provider": "kimi", "providerName": "月之暗面", "vision": true, "default": true, "pricing": { "input": 4.0, "output": 21.0 } },
        
        { "id": "MiniMax-M2.7", "modelId": "MiniMax-M2.7", "displayName": "MiniMax M2.7", "label": "MiniMax M2.7", "provider": "minimax", "providerName": "MiniMax", "vision": false, "default": true, "pricing": { "input": 0.0, "output": 0.0 } }
    ]);

    let mut pool_arr = static_pool.as_array().cloned().unwrap_or_default();
    
    // 从配置中读取 customModels 并追加
    if let Some(custom_models) = super::read_config().get("customModels").and_then(|v| v.as_array()) {
        for cm in custom_models {
            pool_arr.push(cm.clone());
        }
    }
    
    json!(pool_arr)
}

pub fn get_active_models() -> Value {
    let config = super::read_config();
    let main = config.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let clerk = config.get("clerkModel").and_then(|v| v.as_str()).unwrap_or("").to_string();
    json!({ "main": main, "clerk": clerk })
}

pub fn assign_model_role(model_id: String, role: String) -> Value {
    let mut config = super::read_config();
    if let Some(obj) = config.as_object_mut() {
        if role == "main" {
            obj.insert("model".to_string(), json!(model_id));
        } else if role == "clerk" {
            obj.insert("clerkModel".to_string(), json!(model_id));
        }
    }
    super::write_config(&config);
    json!({ "ok": true })
}

pub fn get_api_keys() -> Value {
    let config = super::read_config();
    if let Some(keys) = config.get("apiKeys") {
        return keys.clone();
    }
    
    // 兼容旧的单一 apiKey 字段
    let mut keys = serde_json::Map::new();
    if let Some(legacy_provider) = config.get("provider").and_then(|v| v.as_str()) {
        if let Some(legacy_key) = config.get("apiKey").and_then(|v| v.as_str()) {
            if !legacy_key.is_empty() {
                keys.insert(legacy_provider.to_string(), json!(legacy_key));
            }
        }
    }
    json!(keys)
}

pub fn set_api_key(provider_id: String, api_key: String) -> Value {
    let mut config = super::read_config();
    let mut keys = get_api_keys();
    
    if let Some(obj) = keys.as_object_mut() {
        if api_key.is_empty() {
            obj.remove(&provider_id);
        } else {
            obj.insert(provider_id.clone(), json!(api_key.clone()));
        }
    }
    
    if let Some(cfg_obj) = config.as_object_mut() {
        cfg_obj.insert("apiKeys".to_string(), keys);
        
        // 如果修改的是当前默认的 provider，同步更新旧的 fallback 字段
        if let Some(current_provider) = cfg_obj.get("provider").and_then(|v| v.as_str()) {
            if current_provider == provider_id && !api_key.is_empty() {
                cfg_obj.insert("apiKey".to_string(), json!(api_key));
            }
        }
    }
    
    super::write_config(&config);
    json!({ "ok": true })
}

pub fn add_custom_model(model_id: String, display_name: String, provider: String, base_url: String, api_key: String) -> Value {
    let mut config = super::read_config();
    let mut custom_models = config.get("customModels").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    
    // 如果存在同名的，先移除
    custom_models.retain(|m| m.get("id").and_then(|v| v.as_str()) != Some(&model_id));
    
    let model = json!({
        "id": model_id.clone(),
        "modelId": model_id,
        "displayName": display_name,
        "label": display_name,
        "provider": provider.clone(),
        "providerName": provider,
        "baseUrl": base_url,
        "apiKey": api_key,
        "vision": true, // 自定义模型默认支持vision，依靠实际能力降级
        "pricing": { "input": 0.0, "output": 0.0 }
    });
    
    custom_models.push(model);
    
    if let Some(cfg_obj) = config.as_object_mut() {
        cfg_obj.insert("customModels".to_string(), json!(custom_models));
    }
    
    super::write_config(&config);
    json!({ "ok": true })
}

pub fn remove_custom_model(model_id: String) -> Value {
    let mut config = super::read_config();
    if let Some(custom_models) = config.get("customModels").and_then(|v| v.as_array()) {
        let mut new_models = custom_models.clone();
        new_models.retain(|m| m.get("id").and_then(|v| v.as_str()) != Some(&model_id));
        if let Some(cfg_obj) = config.as_object_mut() {
            cfg_obj.insert("customModels".to_string(), json!(new_models));
        }
        super::write_config(&config);
    }
    json!({ "ok": true })
}

/// 从 config.json 中读取 LLM 配置，包含动态 provider 解析
pub(crate) fn read_llm_config_for_model(model_id: &str) -> (String, String, String, String) {
    let config = super::read_config();
    
    // 1. 根据 model_id 从全局模型池反查 provider
    let pool = get_model_pool();
    let mut dynamic_provider = String::new();
    let mut custom_base_url = None;
    let mut custom_api_key = None;
    
    if let Some(arr) = pool.as_array() {
        if let Some(model_info) = arr.iter().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id)) {
            if let Some(p) = model_info.get("provider").and_then(|v| v.as_str()) {
                dynamic_provider = p.to_string();
            }
            if let Some(url) = model_info.get("baseUrl").and_then(|v| v.as_str()) {
                custom_base_url = Some(url.to_string());
            }
            if let Some(key) = model_info.get("apiKey").and_then(|v| v.as_str()) {
                custom_api_key = Some(key.to_string());
            }
        }
    }
    
    // 如果没查到，降级使用全局配置中的 provider
    let provider = if !dynamic_provider.is_empty() {
        dynamic_provider
    } else {
        config.get("provider").and_then(|v| v.as_str()).unwrap_or("deepseek").to_string()
    };
    
    // 2. 从 apiKeys 对象中获取对应 provider 的 Key (优先使用自定义模型自带的 apiKey 和 baseUrl)
    let api_keys_map = get_api_keys();
    let api_key = custom_api_key.unwrap_or_else(|| {
        api_keys_map.get(&provider).and_then(|v| v.as_str()).unwrap_or("").to_string()
    });
    let mut base_url = custom_base_url.unwrap_or_else(|| {
        config.get("baseURL").and_then(|v| v.as_str()).unwrap_or("").to_string()
    });
    
    // 如果没有配置自定义代理（即当前 baseURL 是某个官方域名或为空），则自动根据 provider 路由到对应的官方地址
    let is_custom_proxy = !base_url.is_empty() && 
        !base_url.contains("deepseek.com") && 
        !base_url.contains("openai.com") && 
        !base_url.contains("aliyuncs.com") && 
        !base_url.contains("volces.com") && 
        !base_url.contains("bigmodel.cn") && 
        !base_url.contains("moonshot.cn") &&
        !base_url.contains("minimax.chat");

    if !is_custom_proxy {
        base_url = match provider.as_str() {
            "minimax" => "https://api.minimax.chat/v1",
            "deepseek" => "https://api.deepseek.com",
            "openai" => "https://api.openai.com/v1",
            "qwen" => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "doubao" => "https://ark.cn-beijing.volces.com/api/v3",
            "zhipu" => "https://open.bigmodel.cn/api/paas/v4",
            "kimi" => "https://api.moonshot.cn/v1",
            "offline" => "http://127.0.0.1:11434/v1",
            _ => "https://api.openai.com/v1",
        }.to_string();
    }
    
    (provider, api_key, model_id.to_string(), base_url)
}

// ═══════════════════════════════════════════════════════════
// 技能摘要生成 (注入 System Prompt)
// ═══════════════════════════════════════════════════════════

/// 构建可用技能的简要列表，供 System Prompt 使用
fn build_skills_summary() -> String {
    let config = super::read_config();
    let skills_dir = match config.get("externalSkillsDir").and_then(|v| v.as_str()) {
        Some(dir) => dir.to_string(),
        None => return String::new(),
    };

    let dir_path = std::path::Path::new(&skills_dir);
    if !dir_path.exists() { return String::new(); }

    let mut lines: Vec<String> = Vec::new();
    lines.push("\n## 可用技能库（可通过 read_skill 加载详细说明）".to_string());

    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() { continue; }
            let skill_md = p.join("SKILL.md");
            if !skill_md.exists() { continue; }

            let folder = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                let (name, desc) = super::tools::parse_skill_frontmatter(&content, &folder);
                let short_desc: String = desc.chars().take(80).collect();
                lines.push(format!("- **{}** ({}): {}", name, folder, short_desc));
            }
        }
    }

    if lines.len() <= 1 { return String::new(); }
    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════
// 记忆引擎摘要生成 (Tier 1 & 2)
// ═══════════════════════════════════════════════════════════

fn build_memory_summary() -> String {
    let data_dir = super::get_data_dir();
    let memory_dir = data_dir.join("memory");
    let soul_path = memory_dir.join("SOUL.md");
    
    let mut lines = Vec::new();

    // 1. 注入 Tier 1: 灵魂
    if soul_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&soul_path) {
            lines.push("\n## 你的记忆系统 (Tier 1: 灵魂)".to_string());
            lines.push(content);
        }
    }

    // 2. 注入 Tier 2: 短期记忆 (最近的 session)
    let sessions_dir = memory_dir.join("sessions");
    if sessions_dir.exists() {
        let mut sessions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().map_or(false, |ext| ext == "json" || ext == "md") {
                    if let Ok(meta) = std::fs::metadata(&p) {
                        if let Ok(modified) = meta.modified() {
                            sessions.push((p, modified));
                        }
                    }
                }
            }
        }
        
        // 按时间倒序，取最近的 3 个
        sessions.sort_by(|a, b| b.1.cmp(&a.1));
        if !sessions.is_empty() {
            lines.push("\n## 短期记忆 (Tier 2: 最近对话摘要)".to_string());
            for (p, _) in sessions.into_iter().take(3) {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if p.extension().map_or(false, |ext| ext == "json") {
                        if let Ok(v) = serde_json::from_str::<Value>(&content) {
                            if let Some(user_topics) = v.get("userTopics").and_then(|t| t.as_array()) {
                                let topics: Vec<String> = user_topics.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect();
                                lines.push(format!("- 讨论过的主题: {}", topics.join(", ")));
                            }
                        }
                    } else {
                        lines.push(content);
                    }
                }
            }
        }
    }

    lines.push("\n## 知识库检索 (Tier 3: 长期记忆)".to_string());
    lines.push("你有权限使用 brain_search 检索知识库，或者使用 write_file 写入 wiki/ 目录来保存新知识。".to_string());
    lines.push("当你产出高质量的分析、对比或总结时，请主动调用 write_file 将其保存到 wiki/ 目录。知识应该不断积累而非消散在聊天记录中。".to_string());

    lines.join("\n")
}

/// 构建 Wiki 知识库状态概览，注入 System Prompt
fn build_wiki_status() -> String {
    let wiki_dir = super::get_wiki_dir();
    let index_path = wiki_dir.join("index.md");

    if !index_path.exists() {
        return "\n## 知识库状态\n知识库为空。当用户拖入文件夹时，引导他们使用知识库构建功能。".to_string();
    }

    let mut lines = Vec::new();
    lines.push("\n## 知识库目录概览".to_string());

    if let Ok(content) = std::fs::read_to_string(&index_path) {
        // 截取前 2000 字符作为快速概览
        let preview: String = content.chars().take(2000).collect();
        lines.push(preview);
        if content.chars().count() > 2000 {
            lines.push("\n... (目录已截断，请使用 brain_search 或 read_file 获取详细信息)".to_string());
        }
    }

    lines.push("\n当用户提出事实性问题时，请先调用 brain_search 检索 Wiki 知识库。如果命中，用 read_file 加载完整页面后再回答，并引用来源路径。".to_string());

    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════
// Tool Calling 引擎 (T-903/T-904)
// ═══════════════════════════════════════════════════════════

/// 内部通用流式处理 — 支持 Tool Calling 循环
async fn stream_internal(
    app: AppHandle,
    messages: Vec<Value>,
) -> Value {
    // 1. 读取 LLM 配置
    let config = super::read_config();
    let config_model_id = config.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (provider, api_key, model_override, custom_base_url) = read_llm_config_for_model(&config_model_id);

    if api_key.is_empty() && provider != "offline" {
        return json!({ "error": "API Key 未配置或被清空，请前往设置检查" });
    }

    // 使用 read_llm_config_for_model 返回的智能路由 base_url（已包含按 provider 自动匹配官方域名的逻辑）
    let base_url = custom_base_url;

    if base_url.is_empty() {
        return json!({ "error": format!("未知的供应商: {}，请检查模型配置", provider) });
    }

    let model_id = if model_override.is_empty() {
        get_default_model(&provider).to_string()
    } else {
        model_override.clone()
    };

    // 2. 构建系统提示词 + 消息
    let has_system = messages.iter().any(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));
    let mut full_messages: Vec<Value> = Vec::new();
    if !has_system {
        let current_dir = std::env::current_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "未知".to_string());
        let os_info = std::env::consts::OS;
        let skills_summary = build_skills_summary();
        let memory_summary = build_memory_summary();
        let wiki_status = build_wiki_status();

        let system_prompt = format!(
            "你是 Bob，一个友善、专业的桌面 AI 私人助手，由 Tauri (Rust) 和 Vue 3 构建。\n\
你当前运行在用户的本地计算机上。\n\
当前操作系统: {}\n\
当前工作目录 (CWD): {}\n\
\n\
请用中文回答用户的问题，并记住你是一个拥有本机访问能力的桌面助手，而不是一个受限的云端网页服务。\n\
\n\
## 工具调用能力\n\
你拥有以下工具，可以主动调用来完成用户的请求：\n\
- **read_file**: 读取用户磁盘上的文本文件\n\
- **list_dir**: 浏览目录内容，帮用户找文件\n\
- **fetch_url**: 抓取网页内容\n\
- **web_search**: 搜索互联网获取实时信息（Tavily/TinyFish 双引擎）\n\
- **write_file**: 将内容写入 wiki/ 目录保存知识\n\
- **append_file**: 向 wiki/index.md 或 wiki/log.md 追加内容\n\
- **brain_search**: 检索你维护的 Wiki 知识库\n\
- **list_skills**: 查看可用的专业分析框架\n\
- **read_skill**: 加载某个技能的详细指南（加载后请严格遵循其工作流程）\n\
\n\
当用户提到文件路径时，请主动调用 read_file 读取；当用户要求分析/规划时，先 read_skill 加载对应框架。\n\
当用户提出事实性问题时，请先调用 brain_search 检索知识库。\n\
{}\n\
{}\n\
{}\n\
## 自主配置能力\n\
当用户要求你帮忙配置 API Key、切换模型、修改主题等系统设置时，请在回复末尾输出 bob-config 代码块：\n\
\n\
```bob-config\n\
{{\\\"op\\\": \\\"set_api_key\\\", \\\"provider\\\": \\\"openai\\\", \\\"value\\\": \\\"sk-xxxx\\\"}}\n\
```\n\
\n\
支持的操作：\n\
- set_api_key: provider 可选 deepseek, openai, qwen, doubao, zhipu, kimi, minimax, tavily, tinyfish\n\
- set_config: key 可选 model, clerkModel, theme, uiScale, language, workspaceDir\n\
\n\
如果用户发送了包含 API Key 的文件或文本，请提取密钥并使用上述格式帮用户配置好。",
            os_info, current_dir, skills_summary, memory_summary, wiki_status
        );

        full_messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    full_messages.extend(messages);

    // 3. 获取工具 Schema
    let tool_schemas = super::tools::get_tool_schemas();

    // 4. 构建 HTTP 客户端
    let url = format!("{}/chat/completions", base_url);
    let client = if provider == "offline" {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    } else {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    };

    // ═══════════════════════════════════════════════════════════
    // Tool Calling 循环 (最多 MAX_TOOL_ROUNDS 轮)
    // ═══════════════════════════════════════════════════════════
    const MAX_TOOL_ROUNDS: usize = 5;
    let mut final_content = String::new();
    let mut final_thinking = String::new();
    let mut final_usage: Option<Value> = None;

    for round in 0..=MAX_TOOL_ROUNDS {
        // 构建请求体
        let mut body = json!({
            "model": model_id,
            "messages": full_messages,
            "stream": true,
        });

        // Minimax 和本地 offline 模型可能不支持 stream_options，会导致 400 报错
        if provider != "minimax" && provider != "offline" {
            body["stream_options"] = json!({ "include_usage": true });
        }

        // 添加工具定义（最后一轮不带工具，防止无限循环）
        if round < MAX_TOOL_ROUNDS && !tool_schemas.is_empty() {
            body["tools"] = json!(tool_schemas);
        }

        // DeepSeek 专有参数
        if provider == "deepseek" {
            body["thinking"] = json!({ "type": "enabled" });
            body["reasoning_effort"] = json!("low");
            body["max_completion_tokens"] = json!(16384);
        } else {
            body["max_tokens"] = json!(8192);
        }

        // 发送请求
        let response = match client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return json!({ "error": format!("网络请求失败: {}", e) }),
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            let safe_text: String = text.chars().take(200).collect();
            let msg = match status {
                401 => "API Key 无效，请在设置中检查".to_string(),
                429 => "请求太频繁，请稍后再试".to_string(),
                _ => format!("API 错误 ({}): {}", status, safe_text),
            };
            return json!({ "error": msg });
        }

        // ── SSE 流式解析（含 Tool Calls 增量累加）────────
        let mut content = String::new();
        let mut thinking_content = String::new();
        let mut usage_data: Option<Value> = None;
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // T-904: Tool call 增量累加器
        struct PendingToolCall { id: String, name: String, arguments: String }
        let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut has_tool_calls = false;

        // <think> 标签解析状态机 (MiniMax/Qwen 等通过 OpenAI 兼容接口返回思考链)
        let mut inside_think_tag = false;
        let mut think_tag_buffer = String::new();

        // ── IPC 降压器 (Debounce Buffer) ────────────────
        // 将高频的逐 Token emit 合并为每 30ms 一次批量发送，
        // 削减约 80% 的跨进程序列化开销，防止前端渲染线程卡顿或 OOM 白屏。
        let mut text_emit_buf = String::new();
        let mut thinking_emit_buf = String::new();
        let mut last_emit_time = std::time::Instant::now();
        const EMIT_INTERVAL_MS: u128 = 30;
        const EMIT_BUFFER_SIZE: usize = 4;

        while let Some(chunk_result) = stream.next().await {
            let bytes = match chunk_result {
                Ok(b) => b,
                Err(_) => break,
            };
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" { continue; }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        // 提取 usage
                        if let Some(usage) = parsed.get("usage") {
                            if usage.is_object() && usage.get("total_tokens").is_some() {
                                usage_data = Some(usage.clone());
                            }
                        }

                        if let Some(choice) = parsed.pointer("/choices/0") {
                            // 检查 finish_reason
                            if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                                if fr == "tool_calls" { has_tool_calls = true; }
                            }

                            if let Some(delta) = choice.get("delta") {
                                // 思考内容 (DeepSeek)
                                if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                                    if !reasoning.is_empty() {
                                        thinking_content.push_str(reasoning);
                                        thinking_emit_buf.push_str(reasoning);
                                    }
                                }
                                // 正文内容（含 <think> 标签解析）
                                if let Some(c) = delta.get("content").and_then(|v| v.as_str()) {
                                    if !c.is_empty() {
                                        // 将当前 chunk 追加到 tag buffer 做标签解析
                                        think_tag_buffer.push_str(c);

                                        // 循环提取 <think> / </think> 边界
                                        loop {
                                            if inside_think_tag {
                                                // 在 <think> 块内部，等待 </think>
                                                if let Some(end_pos) = think_tag_buffer.find("</think>") {
                                                    let thinking_chunk = &think_tag_buffer[..end_pos];
                                                    if !thinking_chunk.is_empty() {
                                                        thinking_content.push_str(thinking_chunk);
                                                        thinking_emit_buf.push_str(thinking_chunk);
                                                    }
                                                    think_tag_buffer = think_tag_buffer[end_pos + 8..].to_string(); // 8 = "</think>".len()
                                                    inside_think_tag = false;
                                                } else {
                                                    // 还没收到闭合标签，全部作为思考内容输出
                                                    let thinking_chunk = think_tag_buffer.clone();
                                                    if !thinking_chunk.is_empty() {
                                                        thinking_content.push_str(&thinking_chunk);
                                                        thinking_emit_buf.push_str(&thinking_chunk);
                                                    }
                                                    think_tag_buffer.clear();
                                                    break;
                                                }
                                            } else {
                                                // 在正文区域，等待 <think>
                                                if let Some(start_pos) = think_tag_buffer.find("<think>") {
                                                    let text_chunk = &think_tag_buffer[..start_pos];
                                                    if !text_chunk.is_empty() {
                                                        content.push_str(text_chunk);
                                                        text_emit_buf.push_str(text_chunk);
                                                    }
                                                    think_tag_buffer = think_tag_buffer[start_pos + 7..].to_string(); // 7 = "<think>".len()
                                                    inside_think_tag = true;
                                                } else {
                                                    // 没有更多标签，全部作为正文输出
                                                    // 但保留尾部可能的不完整标签 (如 "<thi")
                                                    let mut safe_len = if think_tag_buffer.len() > 7 {
                                                        think_tag_buffer.len() - 7
                                                    } else {
                                                        0
                                                    };
                                                    // 回退到最近的 UTF-8 字符边界，防止中文等多字节字符被从中间切开导致 panic
                                                    while safe_len > 0 && !think_tag_buffer.is_char_boundary(safe_len) {
                                                        safe_len -= 1;
                                                    }
                                                    if safe_len > 0 {
                                                        let text_chunk = &think_tag_buffer[..safe_len];
                                                        content.push_str(text_chunk);
                                                        text_emit_buf.push_str(text_chunk);
                                                        think_tag_buffer = think_tag_buffer[safe_len..].to_string();
                                                    }
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                // T-904: Tool calls 增量解析
                                if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                                    for tc in tcs {
                                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        while pending_tool_calls.len() <= idx {
                                            pending_tool_calls.push(PendingToolCall {
                                                id: String::new(), name: String::new(), arguments: String::new(),
                                            });
                                        }
                                        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                            pending_tool_calls[idx].id = id.to_string();
                                        }
                                        if let Some(func) = tc.get("function") {
                                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                                pending_tool_calls[idx].name = name.to_string();
                                            }
                                            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                                pending_tool_calls[idx].arguments.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── IPC 降压: 定时/定量刷新 ────────────────
            let elapsed = last_emit_time.elapsed().as_millis();
            if elapsed >= EMIT_INTERVAL_MS || text_emit_buf.len() >= EMIT_BUFFER_SIZE || thinking_emit_buf.len() >= EMIT_BUFFER_SIZE {
                if !thinking_emit_buf.is_empty() {
                    let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": &thinking_emit_buf }));
                    thinking_emit_buf.clear();
                }
                if !text_emit_buf.is_empty() {
                    let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &text_emit_buf }));
                    text_emit_buf.clear();
                }
                last_emit_time = std::time::Instant::now();
            }
        }

        // ── 流结束: 刷出所有 buffer 残余 ────────────────
        if !thinking_emit_buf.is_empty() {
            let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": &thinking_emit_buf }));
            thinking_emit_buf.clear();
        }
        if !text_emit_buf.is_empty() {
            let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &text_emit_buf }));
            text_emit_buf.clear();
        }

        // 流结束后，将 think_tag_buffer 中残留内容全部输出为正文
        if !think_tag_buffer.is_empty() {
            if inside_think_tag {
                // 残留在 <think> 块中的内容，归入思考
                thinking_content.push_str(&think_tag_buffer);
                let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": &think_tag_buffer }));
            } else {
                content.push_str(&think_tag_buffer);
                let _ = app.emit("llm:chunk", json!({ "type": "text", "content": &think_tag_buffer }));
            }
            think_tag_buffer.clear();
        }

        // ── 防御性检测: DSML/内部标记泄漏 ────────────────
        // 某些供应商（DeepSeek/MiniMax）偶发性地将内部 tool_calls 标记
        // 作为普通文本返回（如 <｜｜DSML｜｜tool_calls>），而非结构化 tool_calls。
        // 检测到后自动清除脏内容并重试本轮请求。
        let dsml_leaked = content.contains('\u{FF5C}') // 全角竖线 ｜
            || content.contains("DSML")
            || content.contains("<|tool_calls|>")
            || content.contains("<|invoke");
        if dsml_leaked && !has_tool_calls && round < MAX_TOOL_ROUNDS {
            log::warn!("DSML token leakage detected in content (round {}), retrying...", round + 1);
            // 通知前端清除已渲染的脏内容
            let _ = app.emit("llm:chunk", json!({ "type": "clear" }));
            let _ = app.emit("llm:chunk", json!({ "type": "thinking", "content": "\n[检测到模型输出异常，自动重试中...]\n" }));
            // 插入提示，引导模型使用标准 function calling 格式
            full_messages.push(json!({
                "role": "system",
                "content": "请使用标准的 function calling JSON 格式调用工具。不要在回复文本中直接输出工具调用标记。"
            }));
            continue; // 回到循环顶部重试
        }

        // ── 判断: Tool Call 还是最终回复 ────────────────
        if has_tool_calls && !pending_tool_calls.is_empty() {
            // 构建 assistant 消息 (含 tool_calls)
            let tc_json: Vec<Value> = pending_tool_calls.iter().map(|tc| json!({
                "id": tc.id, "type": "function",
                "function": { "name": tc.name, "arguments": tc.arguments }
            })).collect();

            let mut assistant_msg = json!({ "role": "assistant", "tool_calls": tc_json });
            if !content.is_empty() { assistant_msg["content"] = json!(content); }
            // DeepSeek thinking 模式要求回传 reasoning_content，否则下一轮请求会 400
            if !thinking_content.is_empty() {
                assistant_msg["reasoning_content"] = json!(thinking_content);
            }
            full_messages.push(assistant_msg);

            // ── 并行执行工具 (tokio::join_all) ──────────────
            let tool_futures: Vec<_> = pending_tool_calls.iter().map(|tc| {
                let app_clone = app.clone();
                let name = tc.name.clone();
                let args_str = tc.arguments.clone();
                async move {
                    let args: Value = serde_json::from_str(&args_str).unwrap_or(json!({}));
                    let result = super::tools::execute_tool(&app_clone, &name, &args).await;
                    (name, result)
                }
            }).collect();

            // 发射所有 tool_start 事件
            for tc in &pending_tool_calls {
                let _ = app.emit("llm:chunk", json!({ "type": "tool_start", "name": tc.name }));
            }

            // 并行等待所有工具完成
            let results = futures_util::future::join_all(tool_futures).await;

            // 按顺序推送结果到 messages
            for (i, (name, result)) in results.into_iter().enumerate() {
                let result_str = serde_json::to_string_pretty(&result).unwrap_or_default();

                // 截断过长结果（防 context window 爆炸），使用 char 边界安全截断
                let truncated = if result_str.len() > 8000 {
                    let mut end = 8000;
                    while end > 0 && !result_str.is_char_boundary(end) { end -= 1; }
                    format!("{}...\n[结果被截断，共 {} 字符]", &result_str[..end], result_str.len())
                } else {
                    result_str
                };

                let _ = app.emit("llm:chunk", json!({ "type": "tool_end", "name": name, "result": truncated }));

                full_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": pending_tool_calls[i].id,
                    "content": truncated
                }));
            }

            log::info!("Tool Calling round {}: executed {} tools", round + 1, pending_tool_calls.len());

            // ── Restatement 注意力重申 (round >= 1) ──────────────────
            // 利用大模型 U 型注意力曲线：对上下文首尾关注度最高、中间最弱。
            // 在第 2 轮起，于 messages 尾部注入 system 消息重申任务约束，
            // 防止模型在消化大量工具结果后"失焦"忘记原始需求。
            if round >= 1 {
                // 提取用户最后一条消息作为任务锚点
                let user_request_hint = full_messages.iter().rev()
                    .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                    .and_then(|m| m.get("content").and_then(|c| c.as_str()))
                    .map(|s| {
                        let chars: String = s.chars().take(200).collect();
                        chars
                    })
                    .unwrap_or_default();

                let restatement = format!(
                    "[注意力重申] 你已经执行了 {} 轮工具调用。请基于以上所有工具返回的结果，\
                    综合回答用户的原始请求。用户的请求是：「{}」\n\
                    请严格遵守你的人设和回复风格规范。不要遗漏关键信息，也不要重复调用已经成功获取结果的工具。",
                    round + 1,
                    user_request_hint
                );

                full_messages.push(json!({
                    "role": "system",
                    "content": restatement
                }));

                log::info!("Restatement injected at round {}", round + 1);
            }

            continue; // 回到循环顶部，将工具结果发回 LLM
        }

        // 没有 tool calls — 这是最终文本回复
        final_content = content;
        final_thinking = thinking_content;
        final_usage = usage_data;
        break;
    }

    // 发送完成事件
    let _ = app.emit("llm:chunk", json!({ "type": "done", "content": "" }));

    let mut pricing = json!({ "input": 0.0, "output": 0.0 });
    let pool = get_model_pool();
    if let Some(arr) = pool.as_array() {
        if let Some(m) = arr.iter().find(|x| x.get("modelId").and_then(|v| v.as_str()) == Some(model_id.as_str())) {
            if let Some(p) = m.get("pricing") {
                pricing = p.clone();
            }
        }
    }

    json!({
        "content": final_content,
        "thinking": if final_thinking.is_empty() { Value::Null } else { Value::String(final_thinking) },
        "usage": final_usage,
        "pricing": pricing,
        "model": model_id,
    })
}

pub async fn stream_chat(app: AppHandle, messages: Vec<Value>) -> Value {
    stream_internal(app, messages).await
}

pub async fn stream_vision(app: AppHandle, mut messages: Vec<Value>, image_base64: String) -> Value {
    if let Some(last) = messages.last_mut() {
        if let Some(obj) = last.as_object_mut() {
            let text_content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("请分析这张图片");
            
            obj.insert("content".to_string(), json!([
                { "type": "text", "text": text_content },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", image_base64),
                        "detail": "auto"
                    }
                }
            ]));
        }
    }
    stream_internal(app, messages).await
}

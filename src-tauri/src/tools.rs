use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::Emitter;

// ═══════════════════════════════════════════════════════════
// T-1401: 工具调用循环熔断器 (Circuit Breaker)
// ═══════════════════════════════════════════════════════════

/// 工具调用追踪器，用于检测并中止重复调用模式
/// 理论依据: 论文命题 2 §3 "状态机异常探测"——在 Agent 推理回路之外引入确定性机制
pub struct ToolCallTracker {
    /// 滚动窗口：最近的 (tool_name, args_hash) 记录
    history: Vec<(String, u64)>,
    /// 总工具调用计数
    total_calls: usize,
    /// 连续相似调用的阈值（超过则熔断）
    repeat_threshold: usize,
    /// 单次对话工具调用总量上限
    budget: usize,
}

impl ToolCallTracker {
    pub fn new() -> Self {
        Self::with_budget(15)
    }

    pub fn with_budget(budget: usize) -> Self {
        Self {
            history: Vec::new(),
            total_calls: 0,
            repeat_threshold: 3,
            budget,
        }
    }

    /// 计算参数的哈希指纹
    fn hash_args(args: &Value) -> u64 {
        let mut hasher = DefaultHasher::new();
        // 序列化为规范化字符串后取哈希
        let s = serde_json::to_string(args).unwrap_or_default();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// 检查即将执行的工具调用是否应被熔断
    /// 返回 Ok(()) 表示放行，Err(reason) 表示应熔断
    pub fn check(&self, name: &str, args: &Value) -> Result<(), String> {
        // 1. 总预算检查
        if self.total_calls >= self.budget {
            return Err(format!(
                "工具调用预算耗尽：本次对话已调用 {} 次工具（上限 {}）。请直接基于已有信息回答用户。",
                self.total_calls, self.budget
            ));
        }

        // 2. 连续重复模式检测
        let current_hash = Self::hash_args(args);
        let recent_same: usize = self
            .history
            .iter()
            .rev()
            .take_while(|(n, h)| n == name && *h == current_hash)
            .count();

        if recent_same >= self.repeat_threshold {
            return Err(format!(
                "循环检测：工具 '{}' 已连续调用 {} 次且参数完全相同，已自动中止。请换一种方式处理或直接告知用户结果。",
                name, recent_same
            ));
        }

        // 3. 同名工具连续调用检测（参数不完全相同但工具名相同）
        let recent_same_name: usize = self
            .history
            .iter()
            .rev()
            .take_while(|(n, _)| n == name)
            .count();

        if recent_same_name >= self.repeat_threshold + 2 {
            return Err(format!(
                "循环检测：工具 '{}' 已连续调用 {} 次（尽管参数略有不同），疑似陷入重试循环，已自动中止。",
                name, recent_same_name
            ));
        }

        Ok(())
    }

    /// 记录一次工具调用
    pub fn record(&mut self, name: &str, args: &Value) {
        let hash = Self::hash_args(args);
        self.history.push((name.to_string(), hash));
        self.total_calls += 1;

        // 保持窗口大小合理（最多 30 条历史）
        if self.history.len() > 30 {
            self.history.drain(..self.history.len() - 30);
        }
    }

    /// 获取总调用次数
    pub fn total(&self) -> usize {
        self.total_calls
    }
}

// ═══════════════════════════════════════════════════════════
// 审计日志 — 记录每次工具调用
// ═══════════════════════════════════════════════════════════

fn audit_tool_call(name: &str, args: &Value, result_summary: &str) {
    let logs_dir = super::get_data_dir().join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let log_path = logs_dir.join("tools.log");

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let args_str: String = serde_json::to_string(args)
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();
    let result_short: String = result_summary.chars().take(100).collect();
    let line = format!(
        "[{}] {} | args: {} | result: {}\n",
        now, name, args_str, result_short
    );

    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

// ═══════════════════════════════════════════════════════════
// 路径白名单 — 判断目标路径是否在允许的写入范围内
// ═══════════════════════════════════════════════════════════

/// 解析 write_file/append_file 的目标路径，基于安全规则返回绝对目标路径
/// - 相对路径 wiki/... → wikiDir
/// - 相对路径 其他 → dataDir
/// - 绝对路径 → 只有在 tracked_folders 或 workspaceDir 内才允许
fn resolve_write_path(path: &str, global_file_access: bool) -> Result<PathBuf, String> {
    // 1. 路径穿越防御 — 使用多层检查防止编码绕过（如 %2e%2e）
    //    先做快速的字符串拦截，再通过 canonicalize 做最终校验
    let decoded_path = urlencoding::decode(path).unwrap_or(std::borrow::Cow::Borrowed(path));
    if decoded_path.contains("..") || path.contains("..") {
        return Err("禁止使用 ../ 进行路径穿越".to_string());
    }

    let p = Path::new(path);

    // 2. 相对路径 — 直接映射到安全目录
    if !p.is_absolute() {
        let config = super::read_config();

        let target = if path.starts_with("wiki/") || path.starts_with("wiki\\") {
            let rel = &path[5..];
            super::get_wiki_dir().join(rel)
        } else if let Some(ws) = config
            .get("workspaceDir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            PathBuf::from(ws).join(p)
        } else {
            super::get_data_dir().join(p)
        };

        // 防御符号链接逃逸：确保解析后的路径仍在安全边界内
        let canon_parent = if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
            fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
        } else {
            target.clone()
        };

        let safe_wiki =
            fs::canonicalize(super::get_wiki_dir()).unwrap_or_else(|_| super::get_wiki_dir());
        let safe_data =
            fs::canonicalize(super::get_data_dir()).unwrap_or_else(|_| super::get_data_dir());
        let safe_ws = config
            .get("workspaceDir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| fs::canonicalize(s).unwrap_or_else(|_| PathBuf::from(s)));

        let is_safe = canon_parent.starts_with(&safe_wiki)
            || canon_parent.starts_with(&safe_data)
            || safe_ws.map_or(false, |ws| canon_parent.starts_with(&ws));

        if !is_safe {
            return Err("路径解析后超出安全边界（可能存在符号链接逃逸）".to_string());
        }

        return Ok(target);
    }

    // 3. 绝对路径 — 需要授权检查
    if global_file_access {
        return Ok(p.to_path_buf()); // 全局文件开关已打开
    }

    // 检查是否在 workspaceDir 或 tracked_folders 内
    let config = super::read_config();
    let mut allowed_dirs: Vec<String> = Vec::new();

    if let Some(ws) = config.get("wikiDir").and_then(|v| v.as_str()) {
        if !ws.is_empty() {
            allowed_dirs.push(ws.to_string());
        }
    }
    if let Some(ws) = config.get("workspaceDir").and_then(|v| v.as_str()) {
        if !ws.is_empty() {
            allowed_dirs.push(ws.to_string());
        }
    }

    // 从数据库读取 tracked_folders
    if let Ok(db) = rusqlite::Connection::open(super::get_data_dir().join("bob.db")) {
        if let Ok(mut stmt) = db.prepare("SELECT folder_path FROM tracked_folders") {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                for r in rows.flatten() {
                    allowed_dirs.push(r);
                }
            }
        }
    }

    let target = p.to_path_buf();

    // 规范化目标路径的父目录（因为目标文件可能尚不存在），防范符号链接攻击
    let target_check_path = if let Some(parent) = target.parent() {
        std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
    } else {
        std::fs::canonicalize(&target).unwrap_or_else(|_| target.clone())
    };
    let target_str = target_check_path.to_string_lossy().to_lowercase();

    for dir in &allowed_dirs {
        // 同样规范化白名单目录
        let canon_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| PathBuf::from(dir));
        let dir_lower = canon_dir.to_string_lossy().to_lowercase();
        if target_str.starts_with(&dir_lower) {
            return Ok(target);
        }
    }

    Err(format!(
        "绝对路径写入被拒绝：{} 不在已关注的目录内。请先将目标文件夹拖入 Bob 添加到关注列表，或打开'全部文件'开关。",
        path
    ))
}

/// 里程碑 9: Tool Calling 引擎
///
/// 本模块定义 Bob 可以"主动使用的手"：
///   - get_tool_schemas()  → 返回 OpenAI Function Calling 格式的工具描述
///   - execute_tool()      → 异步执行工具并返回结果

// ═══════════════════════════════════════════════════════════
// T-901: 工具 Schema 注册表
// ═══════════════════════════════════════════════════════════

/// 返回所有可用工具的 OpenAI Function Calling 格式描述
pub fn get_tool_schemas() -> Vec<Value> {
    let tools = get_builtin_tool_schemas();
    // MCP 工具在异步上下文中合并，这里先返回内建工具
    tools
}

/// 合并 MCP + 原生连接器工具的异步版本
pub async fn get_tool_schemas_with_mcp() -> Vec<Value> {
    let mut tools = get_builtin_tool_schemas();
    // MCP 扩展工具
    let mcp_tools = super::mcp::get_manager().get_all_tool_schemas().await;
    tools.extend(mcp_tools);
    // 原生连接器工具（根据连接状态动态返回）
    tools.extend(super::lark::get_tool_schemas());
    tools.extend(super::google_calendar::get_tool_schemas());
    tools.extend(super::gmail::get_tool_schemas());
    tools
}

fn get_builtin_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "create_directory",
                "description": "创建新文件夹（目录）。如果父目录不存在会自动创建。如果目录已存在，不会报错。需要干活模式授权。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "要创建的目录的绝对路径" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "move_file",
                "description": "移动文件或文件夹到新位置。也可用于跨目录移动。如果目标已存在，将被覆盖。需要干活模式授权。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "源文件或文件夹的绝对路径" },
                        "destination": { "type": "string", "description": "目标路径的绝对路径" }
                    },
                    "required": ["source", "destination"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "copy_file",
                "description": "复制文件到新位置。如果目标已存在，将被覆盖。仅支持复制单个文件。需要干活模式授权。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "源文件的绝对路径" },
                        "destination": { "type": "string", "description": "目标文件的绝对路径" }
                    },
                    "required": ["source", "destination"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "delete_file",
                "description": "安全删除文件或目录（移动到系统回收站）。如果可能，请首选此工具而非直接覆盖。需要干活模式授权。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "要删除的文件或目录的绝对路径" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "rename_file",
                "description": "重命名文件或目录。必须在同一个目录下进行操作（不能跨目录，跨目录请用 move_file）。需要干活模式授权。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "要重命名的原绝对路径" },
                        "new_name": { "type": "string", "description": "新的名称（仅文件名，不是完整路径）" }
                    },
                    "required": ["path", "new_name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "读取指定路径的文件内容。支持 txt/md/json/yaml/csv 等纯文本格式，以及 pdf/docx/pptx/xlsx 等文档格式，文本自动提取，上限 500KB。用于查看用户提到的文件、读取配置文件、提取密钥、解析文档等。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "文件的绝对路径，如 D:\\docs\\keys.txt" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "列出指定目录下的文件和子目录。返回每个条目的名称、类型（文件/目录）和大小。用于帮用户在磁盘上查找文件。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "目录的绝对路径" },
                        "max_items": { "type": "integer", "description": "最多返回的条目数，默认 50" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "fetch_url",
                "description": "抓取指定 URL 的网页内容并提取纯文本。自动去除 HTML 标签、脚本和样式。上限 2MB，超时 10 秒。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "要抓取的网页 URL (http/https)" }
                    },
                    "required": ["url"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_skills",
                "description": "列出所有可用的认知技能（Skill）。每个技能包含名称和简要描述。当你需要查看有哪些可用的分析框架、工作流模板时调用此工具。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_skill",
                "description": "读取指定技能的完整 SKILL.md 文档。当你需要按照某个专业框架（如 SWOT 分析、麦肯锡咨询、旅行规划等）执行任务时，先调用此工具加载框架说明。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill_name": { "type": "string", "description": "技能文件夹名称，如 brainstorming、travel_planner" }
                    },
                    "required": ["skill_name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "搜索互联网获取实时信息。输入搜索关键词，返回相关网页的标题、摘要和链接。当用户询问最新资讯、事实查证、不确定的知识时调用此工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "搜索关键词" },
                        "max_results": { "type": "integer", "description": "最大返回结果数，默认 5" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "system_time",
                "description": "获取当前系统时间（含时区、星期）。当用户询问当前时间、今天星期几等问题时调用此工具。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "查询指定城市的实时天气及预报。当用户询问天气、是否需要带伞、穿搭建议时调用此工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string", "description": "城市名称，如 '深圳'" }
                    },
                    "required": ["city"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "将文本内容安全地写入到指定文件。主要用于整理知识到 wiki/ 目录。例如 path='wiki/projects/万象龙华.md'。（会自动创建不存在的父文件夹，无需预先 create_directory）",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "相对路径或绝对路径，如 'wiki/项目A.md'" },
                        "content": { "type": "string", "description": "要写入的文件内容" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "brain_search",
                "description": "检索你的长期知识库 (wiki/ 目录) 中的内容。当用户询问你之前保存过什么资料、或者某个项目的历史信息时调用此工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "搜索关键词" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "build_knowledge_base",
                "description": "将一个指定路径的文件或文件夹交由后台知识库系统处理。该系统会静默使用牛马模型（Clerk）对大文件进行段落拆解、总结并写入知识图谱。当你被要求“整理某文件进知识库”时，绝对不要自己阅读并总结，而是直接调用本工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "要整理进知识库的文件夹或文件路径" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "append_file",
                "description": "向文件末尾追加内容（不覆盖已有内容）。主要用于更新 wiki/index.md 和 wiki/log.md。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "相对路径，如 'wiki/index.md'" },
                        "content": { "type": "string", "description": "要追加的文本内容" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_calendar_events",
                "description": "列出用户的本地日程表（日历）和待办事项（未完成的）。当用户问'我今天有什么事'、'我的待办事项有哪些'时调用此工具。",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "add_calendar_event",
                "description": "向用户的日程表（日历）中添加一条新的日程或待办事项。当用户要求你记录日程、安排时间、提醒某事或去某个地方时调用此工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "事件的简短标题" },
                        "type": { "type": "string", "description": "事件类型：'event'（日程）或 'todo'（待办）", "enum": ["event", "todo"] },
                        "date": { "type": "string", "description": "日期，格式 YYYY-MM-DD" },
                        "startTime": { "type": "string", "description": "开始时间，格式 HH:MM（可选）" },
                        "endTime": { "type": "string", "description": "结束时间，格式 HH:MM（可选）" },
                        "description": { "type": "string", "description": "详细描述或补充说明" }
                    },
                    "required": ["title", "type", "date"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_model_registry",
                "description": "读取当前模型注册表，返回所有供应商及其模型列表。当你需要查看、对比或更新模型配置时调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "provider_id": { "type": "string", "description": "(可选) 只返回指定供应商，如 'qwen'、'deepseek'" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "test_model_endpoint",
                "description": "测试某个模型 ID 是否能正常调用 API。发送一条简单消息验证连通性。必须在更新注册表前使用此工具验证新模型。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "provider_id": { "type": "string", "description": "供应商 ID，如 'qwen'、'deepseek'" },
                        "model_id": { "type": "string", "description": "要测试的模型 ID" }
                    },
                    "required": ["provider_id", "model_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "update_model_registry",
                "description": "更新指定供应商的模型列表。必须先用 test_model_endpoint 验证新模型可用后才能调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "provider_id": { "type": "string", "description": "供应商 ID" },
                        "models": {
                            "type": "array",
                            "description": "完整的模型列表",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "name": { "type": "string" },
                                    "vision": { "type": "boolean" },
                                    "default": { "type": "boolean" },
                                    "pricing": { "type": "object" }
                                },
                                "required": ["id", "name"]
                            }
                        }
                    },
                    "required": ["provider_id", "models"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "browse_page",
                "description": "使用本机浏览器打开网页并提取渲染后的内容。比 fetch_url 更强大：支持 JS 动态页面、可点击按钮展开内容、绕过反爬机制。当 fetch_url 失败或页面内容明显不完整时，自动改用此工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "要访问的网页 URL" },
                        "wait_seconds": { "type": "integer", "description": "等待页面加载的秒数，默认 3" },
                        "click_selector": { "type": "string", "description": "可选：页面加载后点击的 CSS 选择器，如 '.read-more' 展开全文" },
                        "extract": { "type": "string", "description": "提取模式：text=纯文本(默认), html=完整HTML, screenshot=截图", "enum": ["text", "html", "screenshot"] }
                    },
                    "required": ["url"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "send_wechat_file",
                "description": "通过微信发送文件或图片给指定用户。支持图片(jpg/png/gif/webp等)和任意文件(pdf/doc/mp4/zip等)。图片以图片消息发送，其余格式以文件消息发送。超过 25MB 的文件会自动生成 HTTP 下载链接并以文本形式发送（无需手动调用 share_file）。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "wxid": { "type": "string", "description": "目标微信用户 ID (wxid)" },
                        "file_path": { "type": "string", "description": "要发送的文件的绝对路径" },
                        "caption": { "type": "string", "description": "可选：随文件一起发送的文字说明" }
                    },
                    "required": ["wxid", "file_path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "share_file",
                "description": "为一个本地文件生成公网可访问的下载链接（通过 Web Drop 加密中继）。适用于需要分享文件、生成外链、或者绕过微信/IM平台大小限制的场景。链接在发送方 Bob 运行期间持续有效。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "要分享的文件的绝对路径" }
                    },
                    "required": ["file_path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "enable_browser",
                "description": "启用本机浏览器增强权限。当 browse_page 提示你需要询问用户开启权限，并且用户回复确认开启后，调用此工具来真正开启该功能。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        // ── T-1211: Cron 定时任务工具 ──────────────────────────
        json!({
            "type": "function",
            "function": {
                "name": "add_cron_job",
                "description": "创建一个定时自动执行的任务。Bob 会在指定时间自动执行 prompt 并将结果记录到日程。适用场景：用户说'每天早上8点帮我播报新闻'、'每周一提醒我写周报'、'每隔5分钟检查一下服务器状态'等。cron_expr 是标准5字段格式：分 时 日 月 周。常见示例：'0 8 * * *'=每天8:00，'0 8 * * 1-5'=工作日8:00，'*/30 * * * *'=每30分钟，'0 9 * * 1'=每周一9:00。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "任务的简短标题，如'早报播报'、'周报提醒'" },
                        "cron_expr": { "type": "string", "description": "标准5字段cron表达式：分 时 日 月 周。例如 '0 8 * * *' 表示每天8:00" },
                        "prompt": { "type": "string", "description": "到时间后 Bob 要执行的指令。写得详细一些，因为执行时没有上下文。例如：'请搜索今日科技新闻，整理出5条最重要的，用简洁的中文播报格式输出'" }
                    },
                    "required": ["title", "cron_expr", "prompt"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_cron_jobs",
                "description": "列出当前所有定时任务（包括已启用和已禁用的）。当用户询问'我有哪些定时任务'、'查看自动任务'时调用。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "remove_cron_job",
                "description": "删除指定的定时任务。需要提供任务 ID（从 list_cron_jobs 获取）。当用户说'取消那个定时任务'、'不要再每天播报了'时，先调用 list_cron_jobs 找到 ID，再调用此工具删除。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "要删除的定时任务 ID" }
                    },
                    "required": ["id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "toggle_cron_job",
                "description": "启用或暂停指定的定时任务。暂停后任务不会被删除，可以随时恢复。当用户说'暂停那个任务'或'恢复那个定时任务'时调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "定时任务 ID" },
                        "enabled": { "type": "boolean", "description": "true=启用, false=暂停" }
                    },
                    "required": ["id", "enabled"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "export_html",
                "description": "生成一份精排版的 HTML 报告文件，可直接在浏览器打开或打印为 PDF",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "输出文件名 (不含扩展名)" },
                        "template": { "type": "string", "description": "模板名称: corporate | academic | dashboard", "enum": ["corporate", "academic", "dashboard"] },
                        "title": { "type": "string", "description": "报告标题" },
                        "content": { "type": "string", "description": "Markdown 格式的报告正文" }
                    },
                    "required": ["filename", "template", "title", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "export_xlsx",
                "description": "将结构化数据导出为 Excel 表格文件",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "输出文件名 (不含扩展名)" },
                        "sheets": {
                            "type": "array",
                            "description": "工作表数据",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "工作表名称" },
                                    "headers": { "type": "array", "items": { "type": "string" }, "description": "列标题数组" },
                                    "rows": { "type": "array", "items": { "type": "array", "items": {} }, "description": "数据行的二维数组 (可以是数字或字符串)" }
                                },
                                "required": ["name", "headers", "rows"]
                            }
                        }
                    },
                    "required": ["filename", "sheets"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "export_docx",
                "description": "将文本内容导出为格式工整的 Word 文档",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "输出文件名 (不含扩展名)" },
                        "content": { "type": "string", "description": "Markdown 格式的文档正文" }
                    },
                    "required": ["filename", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "export_pptx",
                "description": "生成 PowerPoint 演示文稿(.pptx)。支持封面页、内容页、章节页和总结页。建议先通过 read_skill 加载 mckinsey-designer 技能来规划 Storyboard 结构。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "输出文件名 (不含扩展名)" },
                        "template": { "type": "string", "description": "配色主题: corporate-dark | corporate-light", "enum": ["corporate-dark", "corporate-light"] },
                        "slides": {
                            "type": "array",
                            "description": "幻灯片数组，每页一个对象",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "type": { "type": "string", "description": "页面类型: cover | content | section | summary" },
                                    "title": { "type": "string", "description": "页面标题" },
                                    "subtitle": { "type": "string", "description": "副标题 (封面页/章节页可用)" },
                                    "content": { "type": "string", "description": "正文段落 (用 \\n\\n 分隔多段)" },
                                    "bullets": { "type": "array", "items": { "type": "string" }, "description": "要点列表" }
                                },
                                "required": ["type", "title"]
                            }
                        }
                    },
                    "required": ["filename", "template", "slides"]
                }
            }
        }),
        // P3-4: save_to_notes — AI可调用的笔记保存工具
        json!({
            "type": "function",
            "function": {
                "name": "save_to_notes",
                "description": "将内容保存为笔记。可用于保存网页摘要、搜索结果精华、AI分析报告等到用户的笔记系统。自动创建带有完整元数据的Markdown笔记。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "笔记标题" },
                        "content": { "type": "string", "description": "笔记正文内容 (Markdown格式)" },
                        "source_url": { "type": "string", "description": "来源URL (可选)" },
                        "source_type": { "type": "string", "enum": ["web_clip", "ai_summary", "research", "reference"], "description": "来源类型" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "标签列表" }
                    },
                    "required": ["title", "content"]
                }
            }
        }),
    ]
}

// ═══════════════════════════════════════════════════════════
// T-902: 工具执行调度器
// ═══════════════════════════════════════════════════════════

/// 执行指定工具并返回结果
/// `from_user`: 当工具调用源自微信会话时，传入消息发送者的加密 wxid
pub async fn execute_tool(
    app: &tauri::AppHandle,
    name: &str,
    args: &Value,
    from_user: Option<&str>,
    global_file_access: bool,
) -> Value {
    // 工具级超时控制：媒体上传类工具给 120 秒，其他给 30 秒
    let timeout_secs = match name {
        "send_wechat_file" => 600, // 大文件上传可能耗时很长，与 CDN 动态超时匹配
        _ => 30,
    };
    let timeout_duration = std::time::Duration::from_secs(timeout_secs);
    let result = match tokio::time::timeout(
        timeout_duration,
        execute_tool_inner(app, name, args, from_user, global_file_access),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => {
            log::warn!("Tool '{}' timed out after {}s", name, timeout_secs);
            json!({ "error": format!("工具 {} 执行超时 (>{}秒)", name, timeout_secs) })
        }
    };
    // 审计日志
    let summary = if let Some(err) = result.get("error") {
        format!("ERROR: {}", err)
    } else if let Some(ok) = result.get("ok") {
        format!("OK: {}", ok)
    } else {
        let s = serde_json::to_string(&result).unwrap_or_default();
        s.chars().take(80).collect()
    };
    audit_tool_call(name, args, &summary);
    result
}

async fn execute_tool_inner(
    app: &tauri::AppHandle,
    name: &str,
    args: &Value,
    from_user: Option<&str>,
    global_file_access: bool,
) -> Value {
    match name {
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            // 路径穿越防御（含 URL 编码绕过防护）
            let decoded = urlencoding::decode(path).unwrap_or(std::borrow::Cow::Borrowed(path));
            if decoded.contains("..") || path.contains("..") {
                return json!({ "error": "禁止使用 ../ 进行路径穿越" });
            }
            match crate::kb_extractor::extract_single_file(std::path::Path::new(path)) {
                Ok(text) => json!({ "content": text }),
                Err(e) => {
                    // Fallback to system_read_file if extraction fails (though extract_single_file already uses it internally for text)
                    let fallback = super::filesystem::system_read_file(path.to_string());
                    if fallback.get("error").is_some() {
                        json!({ "error": format!("文件解析失败: {}", e) })
                    } else {
                        fallback
                    }
                }
            }
        }
        "create_directory" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            tool_create_directory(path, global_file_access).await
        }
        "move_file" => {
            let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let destination = args
                .get("destination")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            tool_move_file(source, destination, global_file_access).await
        }
        "copy_file" => {
            let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let destination = args
                .get("destination")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            tool_copy_file(source, destination, global_file_access).await
        }
        "delete_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            tool_delete_file(path, global_file_access).await
        }
        "rename_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let new_name = args.get("new_name").and_then(|v| v.as_str()).unwrap_or("");
            tool_rename_file(path, new_name, global_file_access).await
        }
        "list_dir" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let max = args.get("max_items").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            tool_list_dir(path, max)
        }
        "fetch_url" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
            super::web::system_fetch_url(url.to_string()).await
        }
        "list_skills" => tool_list_skills(),
        "read_skill" => {
            let skill_name = args
                .get("skill_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            tool_read_skill(skill_name)
        }
        "web_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let max = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;
            tool_web_search(query, max).await
        }
        "system_time" => tool_system_time(),
        "get_weather" => {
            let city = args.get("city").and_then(|v| v.as_str()).unwrap_or("");
            tool_get_weather(city).await
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            tool_write_file(path, content, global_file_access).await
        }
        "brain_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            tool_brain_search(query).await
        }
        "append_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            tool_append_file(path, content, global_file_access).await
        }
        "list_calendar_events" => tool_list_calendar_events(app),
        "add_calendar_event" => tool_add_calendar_event(app, args),
        "build_knowledge_base" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            // 直接触发异步的知识库构建引擎
            super::kb_indexer::system_build_kb(app.clone(), path.to_string(), "clerk".to_string())
                .await;
            json!({
                "status": "success",
                "message": format!("已成功将文件/文件夹 '{}' 发送给后台知识库处理管线。你不需要再做任何事，请告诉用户你已经安排后台在处理了。", path)
            })
        }
        "read_model_registry" => {
            let provider_id = args
                .get("provider_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            tool_read_model_registry(provider_id)
        }
        "test_model_endpoint" => {
            let provider_id = args
                .get("provider_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let model_id = args.get("model_id").and_then(|v| v.as_str()).unwrap_or("");
            tool_test_model_endpoint(provider_id, model_id).await
        }
        "update_model_registry" => {
            let provider_id = args
                .get("provider_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let models = args.get("models").cloned().unwrap_or(json!([]));
            tool_update_model_registry(provider_id, models)
        }
        "browse_page" => tool_browse_page(app, args).await,
        "send_wechat_file" => {
            let llm_wxid = args.get("wxid").and_then(|v| v.as_str()).unwrap_or("");
            // 如果 LLM 传来的 wxid 不是加密格式 (不含 @im.wechat)，
            // 但我们有来自微信会话的真实加密 wxid，则使用后者。
            // 这修复了 LLM 使用用户明文微信号（如 "wobushuai872834"）
            // 而非 ilink API 要求的加密 wxid 的问题。
            let wxid = if !llm_wxid.contains("@im.wechat") {
                if let Some(real_wxid) = from_user {
                    log::info!(
                        "[tools] send_wechat_file: LLM passed '{}', overriding with from_user '{}'",
                        llm_wxid,
                        real_wxid
                    );
                    real_wxid
                } else {
                    llm_wxid
                }
            } else {
                llm_wxid
            };
            let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let caption = args.get("caption").and_then(|v| v.as_str());
            match super::wechat::commands::send_wechat_file(wxid, file_path, caption, app).await {
                Ok(msg) => json!({ "ok": msg }),
                Err(e) => json!({ "error": e }),
            }
        }
        "share_file" => {
            let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            match super::web_drop::start_web_drop(file_path.to_string()).await {
                Ok(url) => {
                    let size_mb = std::fs::metadata(file_path)
                        .map(|m| m.len() as f64 / 1024.0 / 1024.0)
                        .unwrap_or(0.0);
                    json!({
                        "ok": true,
                        "download_url": url,
                        "file_path": file_path,
                        "size_mb": format!("{:.2}", size_mb),
                        "note": "请将 download_url 的完整内容以下面的格式展示给用户，不要自创 Markdown 链接，直接输出纯文本 URL：\n\n💻 电脑/可点击端：直接点击打开下方链接：\n[此处填入完全原生的 download_url]\n\n📱 手机端：如果上方链接无法在微信中直接点击或被截断，请长按复制下方框内的完整内容到浏览器打开：\n```text\n[此处填入完全原生的 download_url]\n```\n\n禁止篡改链接的任何部分（包括 ?v=2 和 #号）。"
                    })
                }
                Err(e) => json!({ "error": e }),
            }
        }
        "enable_browser" => {
            super::browser::enable_browser();
            let detected = super::browser::detect_browser();
            json!({
                "ok": true,
                "message": "浏览器增强已启用",
                "browser_detected": detected.is_some()
            })
        }
        // ── T-1211: Cron 定时任务 ──
        "add_cron_job" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cron_expr = args
                .get("cron_expr")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let prompt = args
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            super::scheduler::system_add_cron_job(title, cron_expr, prompt).await
        }
        "list_cron_jobs" => super::scheduler::system_list_cron_jobs().await,
        "remove_cron_job" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            super::scheduler::system_remove_cron_job(id).await
        }
        "toggle_cron_job" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let enabled = args
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            super::scheduler::system_toggle_cron_job(id, enabled).await
        }
        // ── MCP 扩展工具路由 ──
        name if name.starts_with("mcp_") => super::mcp::get_manager().call_tool(name, args).await,
        // ── 原生连接器工具路由 ──
        name if name.starts_with("lark_") => super::lark::execute_tool(name, args).await,
        name if name.starts_with("google_calendar_") => {
            super::google_calendar::execute_tool(name, args).await
        }
        name if name.starts_with("gmail_") => super::gmail::execute_tool(name, args).await,
        // ── 里程碑 15: 文档输出引擎 ──
        "export_html" => {
            let filename = args
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("report");
            let template = args
                .get("template")
                .and_then(|v| v.as_str())
                .unwrap_or("corporate");
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Report");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

            let data = super::exports::report::ReportData {
                title: title.to_string(),
                template: template.to_string(),
                content: content.to_string(),
            };

            let html = super::exports::report::generate_html_report(&data);

            // 写入默认 exports 目录
            let config = super::read_config();
            let exports_dir = config
                .get("exportsDir")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::desktop_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Bob-Exports")
                });

            let _ = fs::create_dir_all(&exports_dir);
            let file_path = exports_dir.join(format!("{}.html", filename));

            match fs::write(&file_path, html) {
                Ok(_) => {
                    let _ = open::that(&file_path);
                    let path_str = file_path.to_string_lossy().to_string();
                    let _ = app.emit(
                        "llm:chunk",
                        json!({ "type": "file_output", "path": &path_str, "conv_id": "" }),
                    );
                    json!({ "ok": format!("HTML 报告已生成: {}", path_str), "path": path_str })
                }
                Err(e) => json!({ "error": format!("无法写入文件: {}", e) }),
            }
        }
        "export_xlsx" => {
            let filename = args
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("data");

            let mut data = super::exports::xlsx::XlsxData { sheets: Vec::new() };
            if let Some(sheets_array) = args.get("sheets").and_then(|v| v.as_array()) {
                for sheet in sheets_array {
                    if let Ok(sheet_data) = serde_json::from_value(sheet.clone()) {
                        data.sheets.push(sheet_data);
                    }
                }
            }

            let config = super::read_config();
            let exports_dir = config
                .get("exportsDir")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::desktop_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Bob-Exports")
                });

            let _ = fs::create_dir_all(&exports_dir);
            let file_path = exports_dir.join(format!("{}.xlsx", filename));

            match super::exports::xlsx::generate_xlsx(&file_path, &data) {
                Ok(_) => {
                    let _ = open::that(&file_path);
                    let path_str = file_path.to_string_lossy().to_string();
                    let _ = app.emit(
                        "llm:chunk",
                        json!({ "type": "file_output", "path": &path_str, "conv_id": "" }),
                    );
                    json!({ "ok": format!("Excel 表格已生成: {}", path_str), "path": path_str })
                }
                Err(e) => json!({ "error": format!("无法生成 Excel: {}", e) }),
            }
        }
        "export_docx" => {
            let filename = args
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("document");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

            let config = super::read_config();
            let exports_dir = config
                .get("exportsDir")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::desktop_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Bob-Exports")
                });

            let _ = fs::create_dir_all(&exports_dir);
            let file_path = exports_dir.join(format!("{}.docx", filename));

            match super::exports::docx::generate_docx(&file_path, content) {
                Ok(_) => {
                    let _ = open::that(&file_path);
                    let path_str = file_path.to_string_lossy().to_string();
                    let _ = app.emit(
                        "llm:chunk",
                        json!({ "type": "file_output", "path": &path_str, "conv_id": "" }),
                    );
                    json!({ "ok": format!("Word 文档已生成: {}", path_str), "path": path_str })
                }
                Err(e) => json!({ "error": format!("无法生成 Word: {}", e) }),
            }
        }
        "export_pptx" => {
            let filename = args
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("presentation");
            let template = args
                .get("template")
                .and_then(|v| v.as_str())
                .unwrap_or("corporate-dark");

            let mut slides = Vec::new();
            if let Some(slides_array) = args.get("slides").and_then(|v| v.as_array()) {
                for slide in slides_array {
                    if let Ok(slide_data) = serde_json::from_value(slide.clone()) {
                        slides.push(slide_data);
                    }
                }
            }

            let data = super::exports::pptx::PptxData {
                template: template.to_string(),
                slides,
            };

            let config = super::read_config();
            let exports_dir = config
                .get("exportsDir")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::desktop_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Bob-Exports")
                });

            let _ = fs::create_dir_all(&exports_dir);
            let file_path = exports_dir.join(format!("{}.pptx", filename));

            match super::exports::pptx::generate_pptx(&file_path, &data) {
                Ok(_) => {
                    let _ = open::that(&file_path);
                    let path_str = file_path.to_string_lossy().to_string();
                    let _ = app.emit(
                        "llm:chunk",
                        json!({ "type": "file_output", "path": &path_str, "conv_id": "" }),
                    );
                    json!({ "ok": format!("PowerPoint 演示文稿已生成: {}", path_str), "path": path_str })
                }
                Err(e) => json!({ "error": format!("无法生成 PPTX: {}", e) }),
            }
        }
        // P3-4: save_to_notes — 将内容保存为笔记
        "save_to_notes" => {
            let title = args["title"].as_str().unwrap_or("Untitled");
            let content = args["content"].as_str().unwrap_or("");
            let source_url = args["source_url"].as_str().unwrap_or("");
            let source_type = args["source_type"].as_str().unwrap_or("reference");
            let tags: Vec<String> = args["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_else(|| vec![source_type.to_string()]);

            // Create the note via notebook module
            match super::notebook::notebook_create_note(
                title.to_string(),
                tags.clone(),
                Some("sources".to_string()),
            ) {
                Ok(res) => {
                    if let Some(path) = res["path"].as_str() {
                        // Build enriched content with source metadata
                        let mut body = String::new();
                        if !source_url.is_empty() {
                            body.push_str(&format!("> 来源: [{}]({})\n\n", source_url, source_url));
                        }
                        body.push_str(content);

                        // Save content (notebook_save_note needs DbState, use file-based approach)
                        let notes_dir = super::notebook::get_notes_dir();
                        let full_path = notes_dir.join(path);
                        if let Ok(existing) = fs::read_to_string(&full_path) {
                            // Append body after frontmatter
                            if existing.starts_with("---") {
                                if let Some(end_idx) = existing[3..].find("---") {
                                    let fm = &existing[..3 + end_idx + 3];
                                    let new_content = format!("{}\n\n{}", fm, body);
                                    let _ = fs::write(&full_path, new_content);
                                }
                            }
                        }

                        json!({ "ok": format!("笔记「{}」已保存到 sources/", title), "path": path })
                    } else {
                        json!({ "error": "创建笔记失败" })
                    }
                }
                Err(e) => json!({ "error": format!("创建笔记失败: {}", e) }),
            }
        }
        _ => json!({ "error": format!("未知工具: {}", name) }),
    }
}

// ═══════════════════════════════════════════════════════════
// 工具实现
// ═══════════════════════════════════════════════════════════

/// list_dir — 列出目录内容
fn tool_list_dir(path: &str, max_items: usize) -> Value {
    let p = Path::new(path);
    if !p.exists() {
        return json!({ "error": format!("路径不存在: {}", path) });
    }
    if !p.is_dir() {
        return json!({ "error": "指定路径不是目录" });
    }

    let entries = match fs::read_dir(p) {
        Ok(e) => e,
        Err(e) => return json!({ "error": format!("无法读取目录: {}", e) }),
    };

    let mut items: Vec<Value> = Vec::new();
    for entry in entries.flatten().take(max_items) {
        let entry_path = entry.path();
        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let is_dir = entry_path.is_dir();
        let size = if !is_dir {
            fs::metadata(&entry_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        items.push(json!({
            "name": name,
            "type": if is_dir { "directory" } else { "file" },
            "size": size
        }));
    }

    json!({
        "path": path,
        "count": items.len(),
        "items": items
    })
}

/// list_skills — 列出可用技能（从 externalSkillsDir 扫描）
fn tool_list_skills() -> Value {
    let config = super::read_config();
    let bundled_dir = config
        .get("bundledSkillsDir")
        .and_then(|v| v.as_str())
        .map(|s| Path::new(s).to_path_buf());
    let external_dir = config
        .get("externalSkillsDir")
        .and_then(|v| v.as_str())
        .map(|s| Path::new(s).to_path_buf());

    let mut skills_map = std::collections::HashMap::new();

    let mut load_from_dir = |dir_opt: Option<&std::path::PathBuf>| {
        if let Some(dir) = dir_opt {
            if dir.exists() && dir.is_dir() {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if !p.is_dir() {
                            continue;
                        }
                        let md = p.join("SKILL.md");
                        if !md.exists() {
                            continue;
                        }
                        let folder_name = p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let (name, desc) = match fs::read_to_string(&md) {
                            Ok(content) => parse_skill_frontmatter(&content, &folder_name),
                            Err(_) => (folder_name.clone(), String::new()),
                        };
                        skills_map.insert(
                            folder_name.clone(),
                            json!({
                                "id": folder_name,
                                "name": name,
                                "description": desc
                            }),
                        );
                    }
                }
            }
        }
    };

    // 先加载内置，再加载外部（外部覆盖内置同名技能）
    load_from_dir(bundled_dir.as_ref());
    load_from_dir(external_dir.as_ref());

    let skills: Vec<Value> = skills_map.into_values().collect();
    json!({ "skills": skills, "count": skills.len() })
}

/// read_skill — 读取指定技能的 SKILL.md 全文
fn tool_read_skill(skill_name: &str) -> Value {
    if skill_name.is_empty() {
        return json!({ "error": "请指定技能名称" });
    }

    // 安全检查：防止路径穿越
    if skill_name.contains("..") || skill_name.contains('/') || skill_name.contains('\\') {
        return json!({ "error": "非法技能名称" });
    }

    let config = super::read_config();
    let skills_dir = match config.get("externalSkillsDir").and_then(|v| v.as_str()) {
        Some(dir) => dir.to_string(),
        None => return json!({ "error": "未配置技能目录 (externalSkillsDir)" }),
    };

    let skill_md = Path::new(&skills_dir).join(skill_name).join("SKILL.md");
    if !skill_md.exists() {
        return json!({ "error": format!("技能 '{}' 不存在", skill_name) });
    }

    match fs::read_to_string(&skill_md) {
        Ok(content) => {
            // 同时检查是否有 references 子目录
            let refs_dir = Path::new(&skills_dir).join(skill_name).join("references");
            let mut ref_files: Vec<String> = Vec::new();
            if refs_dir.exists() {
                if let Ok(entries) = fs::read_dir(&refs_dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                            ref_files.push(name.to_string());
                        }
                    }
                }
            }

            json!({
                "skill_name": skill_name,
                "content": content,
                "reference_files": ref_files
            })
        }
        Err(e) => json!({ "error": format!("读取失败: {}", e) }),
    }
}

// ═══════════════════════════════════════════════════════════
// web_search — Tavily (主) + TinyFish (降级) 双引擎搜索
// ═══════════════════════════════════════════════════════════

/// Web Search: 先尝试 Tavily，失败后降级 TinyFish
async fn tool_web_search(query: &str, max_results: usize) -> Value {
    if query.is_empty() {
        return json!({ "error": "搜索关键词不能为空" });
    }

    let config = super::read_config();
    let api_keys = config.get("apiKeys").cloned().unwrap_or(json!({}));
    // 1. 尝试 Tavily
    let tavily_key = api_keys
        .get("TAVILY_API_KEY")
        .or_else(|| api_keys.get("tavily"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !tavily_key.is_empty() {
        if let Ok(result) = search_tavily(query, max_results, tavily_key).await {
            return result;
        }
    }

    // 2. 降级 TinyFish (免费)
    let tinyfish_key = api_keys
        .get("TINYFISH_API_KEY")
        .or_else(|| api_keys.get("tinyfish"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !tinyfish_key.is_empty() {
        if let Ok(result) = search_tinyfish(query, max_results, tinyfish_key).await {
            return result;
        }
    }

    json!({ "error": "搜索失败：未配置 Tavily 或 TinyFish API Key。请在设置中添加，或告诉我密钥。" })
}

/// Tavily Search API (POST JSON, api_key in body)
async fn search_tavily(query: &str, max_results: usize, api_key: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {}", e))?;

    let body = json!({
        "api_key": api_key,
        "query": query,
        "max_results": max_results,
        "include_answer": true,
        "search_depth": "basic"
    });

    let resp = client
        .post("https://api.tavily.com/search")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Tavily 请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Tavily HTTP {}", resp.status()));
    }

    let data: Value = resp.json().await.map_err(|e| format!("解析失败: {}", e))?;

    // 提取结构化结果
    let mut results: Vec<Value> = Vec::new();
    if let Some(items) = data.get("results").and_then(|v| v.as_array()) {
        for item in items.iter().take(max_results) {
            results.push(json!({
                "title": item.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "url": item.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                "snippet": item.get("content").and_then(|v| v.as_str()).unwrap_or("")
            }));
        }
    }

    let answer = data.get("answer").and_then(|v| v.as_str()).unwrap_or("");

    Ok(json!({
        "engine": "tavily",
        "query": query,
        "answer": answer,
        "results": results,
        "count": results.len()
    }))
}

/// TinyFish Search API (GET, X-API-Key header)
async fn search_tinyfish(query: &str, max_results: usize, api_key: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {}", e))?;
    // URL-safe 编码查询参数
    let encoded_query: String = query
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect();
    let search_url = format!(
        "https://api.search.tinyfish.ai?q={}&max_results={}",
        encoded_query, max_results
    );

    let resp = client
        .get(&search_url)
        .header("X-API-Key", api_key)
        .send()
        .await
        .map_err(|e| format!("TinyFish 请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("TinyFish HTTP {}", resp.status()));
    }

    let data: Value = resp.json().await.map_err(|e| format!("解析失败: {}", e))?;

    let mut results: Vec<Value> = Vec::new();
    if let Some(items) = data.get("results").and_then(|v| v.as_array()) {
        for item in items.iter().take(max_results) {
            results.push(json!({
                "title": item.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "url": item.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                "snippet": item.get("snippet").or(item.get("content")).and_then(|v| v.as_str()).unwrap_or("")
            }));
        }
    }

    Ok(json!({
        "engine": "tinyfish",
        "query": query,
        "results": results,
        "count": results.len()
    }))
}

/// 解析 SKILL.md 的 YAML frontmatter (公开供 llm.rs 调用)
pub fn parse_skill_frontmatter(content: &str, fallback_name: &str) -> (String, String) {
    let mut name = fallback_name.to_string();
    let mut description = String::new();

    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        if let Some(first_line) = trimmed.lines().next() {
            let clean = first_line.trim_start_matches('#').trim();
            if !clean.is_empty() {
                name = clean.to_string();
            }
        }
    } else {
        let after_first = &trimmed[3..];
        if let Some(end_pos) = after_first.find("\n---") {
            let yaml_block = &after_first[..end_pos];
            for line in yaml_block.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("name:") {
                    name = val.trim().trim_matches('"').trim_matches('\'').to_string();
                } else if let Some(val) = line.strip_prefix("description:") {
                    description = val.trim().trim_matches('"').trim_matches('\'').to_string();
                    // 截断超长描述
                    if description.len() > 200 {
                        let truncated: String = description.chars().take(200).collect();
                        description = format!("{}...", truncated);
                    }
                }
            }
        }
    }

    // Strip common emoji ranges from the name
    name = name
        .chars()
        .filter(|c| {
            let cp = *c as u32;
            // Exclude common emoji blocks:
            // 0x2600-0x27BF (Misc Symbols, Dingbats)
            // 0x1F300-0x1FAFF (Pictographs, Emoticons, Symbols)
            !((cp >= 0x2600 && cp <= 0x27BF) || (cp >= 0x1F300 && cp <= 0x1FAFF))
        })
        .collect();
    name = name.trim().to_string();

    if name.to_uppercase().starts_with("SKILL:") {
        name = name[6..].trim().to_string();
    }

    (name, description)
}

// ═══════════════════════════════════════════════════════════
// T-1001: 环境感知与记忆工具
// ═══════════════════════════════════════════════════════════

fn tool_system_time() -> Value {
    let now = chrono::Local::now();
    json!({
        "datetime": now.format("%Y-%m-%d %H:%M:%S").to_string(),
        "timezone": now.format("%Z").to_string(),
        "weekday": now.format("%A").to_string(),
        "date": now.format("%Y-%m-%d").to_string(),
        "time": now.format("%H:%M:%S").to_string()
    })
}

async fn tool_get_weather(city: &str) -> Value {
    let geo_url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=zh",
        city
    );
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("HTTP 客户端创建失败: {}", e) }),
    };

    let geo_resp = match client.get(&geo_url).send().await {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("获取地理位置失败: {}", e) }),
    };

    let geo_json: Value = match geo_resp.json().await {
        Ok(j) => j,
        Err(_) => return json!({ "error": "无法解析地理位置数据" }),
    };

    let results = geo_json.get("results").and_then(|v| v.as_array());
    let location = match results.and_then(|arr| arr.first()) {
        Some(loc) => loc,
        None => return json!({ "error": format!("找不到城市: {}", city) }),
    };
    let lat = location
        .get("latitude")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let lon = location
        .get("longitude")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let resolved_name = location
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(city);

    let weather_url = format!("https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,wind_speed_10m&daily=temperature_2m_max,temperature_2m_min&timezone=Asia%2FShanghai", lat, lon);

    let weather_resp = match client.get(&weather_url).send().await {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("获取天气数据失败: {}", e) }),
    };

    let weather_json: Value = match weather_resp.json().await {
        Ok(j) => j,
        Err(_) => return json!({ "error": "无法解析天气数据" }),
    };

    let current = weather_json.get("current").cloned().unwrap_or(json!({}));
    let daily = weather_json.get("daily").cloned().unwrap_or(json!({}));

    let weather_code = current
        .get("weather_code")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let condition = match weather_code {
        0 => "☀️ 晴朗",
        1..=3 => "⛅ 多云/阴天",
        45..=48 => "🌫️ 雾",
        51..=55 => "🌧️ 阵雨",
        61..=65 => "🌧️ 雨",
        71..=75 => "❄️ 雪",
        95..=99 => "⛈️ 雷雨",
        _ => "未知",
    };

    let temp = current
        .get("temperature_2m")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let max_temp = daily
        .get("temperature_2m_max")
        .and_then(|a| a.as_array())
        .and_then(|a| a.get(0))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let min_temp = daily
        .get("temperature_2m_min")
        .and_then(|a| a.as_array())
        .and_then(|a| a.get(0))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    json!({
        "city": resolved_name,
        "temperature": temp,
        "max_temperature": max_temp,
        "min_temperature": min_temp,
        "condition": condition,
        "wind_speed": current.get("wind_speed_10m").and_then(|v| v.as_f64()).unwrap_or(0.0)
    })
}

async fn tool_write_file(path: &str, content: &str, global_file_access: bool) -> Value {
    let target_path = match resolve_write_path(path, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": e }),
    };

    if let Some(parent) = target_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    match fs::write(&target_path, content) {
        Ok(_) => {
            json!({ "ok": true, "path": target_path.to_string_lossy().to_string(), "bytes_written": content.len() })
        }
        Err(e) => json!({ "error": format!("写入文件失败: {}", e) }),
    }
}

async fn tool_append_file(path: &str, content: &str, global_file_access: bool) -> Value {
    let target_path = match resolve_write_path(path, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": e }),
    };

    if let Some(parent) = target_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // 读取现有内容（如果文件存在）
    let existing = fs::read_to_string(&target_path).unwrap_or_default();
    let new_content = format!("{}{}", existing, content);

    match fs::write(&target_path, &new_content) {
        Ok(_) => {
            json!({ "ok": true, "path": target_path.to_string_lossy().to_string(), "bytes_appended": content.len() })
        }
        Err(e) => json!({ "error": format!("追加文件失败: {}", e) }),
    }
}

async fn tool_create_directory(path: &str, global_file_access: bool) -> Value {
    let target_path = match resolve_write_path(path, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": e }),
    };
    match fs::create_dir_all(&target_path) {
        Ok(_) => json!({ "ok": true, "path": target_path.to_string_lossy().to_string() }),
        Err(e) => json!({ "error": format!("创建文件夹失败: {}", e) }),
    }
}

async fn tool_move_file(source: &str, destination: &str, global_file_access: bool) -> Value {
    let src_path = match resolve_write_path(source, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": format!("源路径: {}", e) }),
    };
    let dst_path = match resolve_write_path(destination, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": format!("目标路径: {}", e) }),
    };
    if let Some(parent) = dst_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::rename(&src_path, &dst_path) {
        Ok(_) => {
            json!({ "ok": true, "source": src_path.to_string_lossy().to_string(), "destination": dst_path.to_string_lossy().to_string() })
        }
        Err(e) => json!({ "error": format!("移动文件失败: {}", e) }),
    }
}

async fn tool_copy_file(source: &str, destination: &str, global_file_access: bool) -> Value {
    let src_path = match resolve_write_path(source, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": format!("源路径: {}", e) }),
    };
    let dst_path = match resolve_write_path(destination, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": format!("目标路径: {}", e) }),
    };
    if let Some(parent) = dst_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::copy(&src_path, &dst_path) {
        Ok(bytes) => {
            json!({ "ok": true, "bytes_copied": bytes, "destination": dst_path.to_string_lossy().to_string() })
        }
        Err(e) => json!({ "error": format!("复制文件失败: {}", e) }),
    }
}

async fn tool_delete_file(path: &str, global_file_access: bool) -> Value {
    let target_path = match resolve_write_path(path, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": e }),
    };
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    match trash::delete(&target_path) {
        Ok(_) => json!({ "ok": true, "path": target_path.to_string_lossy().to_string() }),
        Err(e) => json!({ "error": format!("放入回收站失败: {}", e) }),
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if target_path.is_dir() {
            match std::fs::remove_dir_all(&target_path) {
                Ok(_) => json!({ "ok": true, "path": target_path.to_string_lossy().to_string() }),
                Err(e) => json!({ "error": format!("直接删除文件夹失败: {}", e) }),
            }
        } else {
            match std::fs::remove_file(&target_path) {
                Ok(_) => json!({ "ok": true, "path": target_path.to_string_lossy().to_string() }),
                Err(e) => json!({ "error": format!("直接删除文件失败: {}", e) }),
            }
        }
    }
}

async fn tool_rename_file(path: &str, new_name: &str, global_file_access: bool) -> Value {
    let target_path = match resolve_write_path(path, global_file_access) {
        Ok(p) => p,
        Err(e) => return json!({ "error": e }),
    };
    if let Some(parent) = target_path.parent() {
        let new_path = parent.join(new_name);
        match fs::rename(&target_path, &new_path) {
            Ok(_) => json!({ "ok": true, "path": new_path.to_string_lossy().to_string() }),
            Err(e) => json!({ "error": format!("重命名失败: {}", e) }),
        }
    } else {
        json!({ "error": "无法确定父目录" })
    }
}

async fn tool_brain_search(query: &str) -> Value {
    // ── RRF (Reciprocal Rank Fusion) 增强检索 ─────────────────
    // Step 1: 从 FTS5 获取 Top 20 候选
    // Step 2: 在 Rust 内存中叠加时间衰减 + 类型权重重排
    // Step 3: 返回 Top 5 给 LLM
    if let Ok(db) = rusqlite::Connection::open(super::get_data_dir().join("bob.db")) {
        let fts_query = query
            .split_whitespace()
            .map(|w| format!("\"{}\"", w))
            .collect::<Vec<_>>()
            .join(" OR ");

        if let Ok(mut stmt) = db.prepare(
            "SELECT file_name, source_path, wiki_path, summary, keywords, category, indexed_at, rank \
             FROM wiki_fts WHERE wiki_fts MATCH ?1 ORDER BY rank LIMIT 20"
        ) {
            if let Ok(rows) = stmt.query_map(rusqlite::params![fts_query], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(), // file_name
                    row.get::<_, String>(1).unwrap_or_default(), // source_path
                    row.get::<_, String>(2).unwrap_or_default(), // wiki_path
                    row.get::<_, String>(3).unwrap_or_default(), // summary
                    row.get::<_, String>(4).unwrap_or_default(), // keywords
                    row.get::<_, String>(5).unwrap_or_default(), // category
                    row.get::<_, String>(6).unwrap_or_default(), // indexed_at
                    row.get::<_, f64>(7).unwrap_or(0.0),         // rank (negative = better)
                ))
            }) {
                let mut candidates: Vec<(f64, serde_json::Value)> = Vec::new();
                let now_ts = chrono::Local::now().timestamp() as f64;

                for row_result in rows.flatten() {
                    let (file_name, source_path, wiki_path, summary, keywords, category, indexed_at, fts_rank) = row_result;

                    // ── RRF 基础分 (FTS5 rank 越负越好，取绝对值) ──
                    let base_score = fts_rank.abs().max(0.001);

                    // ── 类型权重 ──────────────────────────────
                    let type_weight = if category.contains("feedback") {
                        2.5  // 用户对 AI 错误的纠正 — 最高优先级
                    } else if category.contains("learned") {
                        1.5  // 自动提取的知识事实
                    } else if wiki_path.contains("superseded") || summary.contains("superseded") {
                        0.3  // 过时记忆 — 大幅降权
                    } else {
                        1.0  // 普通 wiki 内容
                    };

                    // ── 时间衰减 (越新越重要) ─────────────────
                    let time_weight = if !indexed_at.is_empty() {
                        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&indexed_at, "%Y-%m-%d %H:%M:%S") {
                            let age_days = (now_ts - dt.and_utc().timestamp() as f64) / 86400.0;
                            // 半衰期 30 天: 30天前的记忆权重约为 0.5
                            (0.5_f64).powf(age_days / 30.0).max(0.1)
                        } else {
                            0.5 // 无法解析日期，给中等权重
                        }
                    } else {
                        0.5
                    };

                    let final_score = base_score * type_weight * time_weight;

                    candidates.push((final_score, json!({
                        "file_name": file_name,
                        "source_path": source_path,
                        "wiki_path": wiki_path,
                        "summary": summary,
                        "keywords": keywords,
                        "category": category,
                        "relevance_score": format!("{:.3}", final_score),
                    })));
                }

                if !candidates.is_empty() {
                    // 按 RRF 得分降序排列，取 Top 5
                    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                    let top_results: Vec<Value> = candidates.into_iter()
                        .take(5)
                        .map(|(_, v)| v)
                        .collect();

                    return json!({
                        "source": "fts5_rrf",
                        "results": top_results,
                        "hint": "使用 read_file 读取 wiki_path 获取完整摘要页内容"
                    });
                }
            }
        }
    }

    // 回退：遍历 wiki/ 目录做简单文本匹配（兼容 FTS5 表为空的情况）
    let wiki_dir = super::get_wiki_dir();
    if !wiki_dir.exists() {
        return json!({ "error": "wiki 知识库尚不存在，没有任何记录。" });
    }

    let mut results = Vec::new();
    let q = query.to_lowercase();

    for entry in walkdir::WalkDir::new(&wiki_dir).into_iter().flatten() {
        let p = entry.path();
        if p.is_file()
            && p.extension()
                .map_or(false, |ext| ext == "md" || ext == "txt")
        {
            if let Ok(content) = fs::read_to_string(p) {
                if content.to_lowercase().contains(&q) {
                    let rel_path = p
                        .strip_prefix(&super::get_data_dir())
                        .unwrap_or(p)
                        .to_string_lossy()
                        .to_string();

                    // 提取匹配的上下文片段
                    let snippet = if let Some(idx) = content.to_lowercase().find(&q) {
                        let start = idx.saturating_sub(40);
                        let end = (idx + q.len() + 80).min(content.len());
                        let slice = &content[start..end];
                        format!("...{}...", slice.replace('\n', " "))
                    } else {
                        String::new()
                    };

                    results.push(json!({
                        "path": rel_path,
                        "snippet": snippet
                    }));
                }
            }
        }
    }

    if results.is_empty() {
        json!({ "message": format!("未在知识库中找到包含 '{}' 的内容。", query) })
    } else {
        json!({ "source": "file_scan", "results": results })
    }
}

/// list_calendar_events — 列出本地日程/待办
fn tool_list_calendar_events(app: &tauri::AppHandle) -> Value {
    use tauri::Manager;
    let db = app.state::<crate::db::DbState>();
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({ "error": "数据库锁失败" }),
    };

    let mut stmt = match conn.prepare(
        "SELECT id, title, type, status, date, start_time, end_time, description 
         FROM events WHERE status != 'done' AND status != 'cancelled' ORDER BY date ASC, start_time ASC"
    ) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("查询失败: {}", e) }),
    };

    let rows = match stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "type": row.get::<_, String>(2)?,
            "status": row.get::<_, String>(3)?,
            "date": row.get::<_, Option<String>>(4).unwrap_or(None),
            "start_time": row.get::<_, Option<String>>(5).unwrap_or(None),
            "end_time": row.get::<_, Option<String>>(6).unwrap_or(None),
            "description": row.get::<_, Option<String>>(7).unwrap_or(None),
        }))
    }) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("解析查询结果失败: {}", e) }),
    };

    let events: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
    if events.is_empty() {
        json!({ "message": "目前没有任何未完成的日程或待办事项。" })
    } else {
        json!({ "events": events })
    }
}

/// add_calendar_event — 添加日程/待办
fn tool_add_calendar_event(app: &tauri::AppHandle, args: &Value) -> Value {
    use tauri::Manager;
    let db = app.state::<crate::db::DbState>();
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({ "error": "数据库锁失败" }),
    };

    let id = format!("evt-{}", super::now_ms());
    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let etype = args.get("type").and_then(|v| v.as_str()).unwrap_or("event");
    let status = "pending";
    let date_str = args.get("date").and_then(|v| v.as_str()).unwrap_or("");
    let mut db_start_time = None;
    let mut db_end_time = None;

    if !date_str.is_empty() {
        if let Some(st) = args.get("startTime").and_then(|v| v.as_str()) {
            if st.contains("-") {
                db_start_time = Some(st.to_string());
            } else {
                let st_clean = if st.len() == 5 {
                    format!("{}:00", st)
                } else {
                    st.to_string()
                };
                db_start_time = Some(format!("{} {}", date_str, st_clean));
            }
        } else {
            db_start_time = Some(format!("{} 00:00:00", date_str));
        }

        if let Some(et) = args.get("endTime").and_then(|v| v.as_str()) {
            if et.contains("-") {
                db_end_time = Some(et.to_string());
            } else {
                let et_clean = if et.len() == 5 {
                    format!("{}:00", et)
                } else {
                    et.to_string()
                };
                db_end_time = Some(format!("{} {}", date_str, et_clean));
            }
        }
    }

    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match conn.execute(
        "INSERT INTO events (id, title, type, status, date, start_time, end_time, description, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![id, title, etype, status, date_str, db_start_time, db_end_time, description, super::now_ms()],
    ) {
        Ok(_) => json!({ "ok": true, "id": id, "message": format!("成功添加日程：{}", title) }),
        Err(e) => json!({ "error": format!("添加日程失败：{}", e) }),
    }
}

// ═══════════════════════════════════════════════════════════
// 模型注册表自维护工具
// ═══════════════════════════════════════════════════════════

/// read_model_registry — 读取模型注册表
fn tool_read_model_registry(provider_id: &str) -> Value {
    let path = super::get_data_dir().join("model_providers.json");
    let registry: Value = if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or(json!({ "error": "JSON 解析失败" }))
    } else {
        return json!({ "error": "model_providers.json 不存在，请重启应用以初始化" });
    };

    if provider_id.is_empty() {
        return registry;
    }

    // 只返回指定供应商
    if let Some(providers) = registry.get("providers").and_then(|v| v.as_array()) {
        if let Some(provider) = providers
            .iter()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(provider_id))
        {
            return provider.clone();
        }
    }

    json!({ "error": format!("未找到供应商: {}", provider_id) })
}

/// test_model_endpoint — 测试模型 API 连通性
async fn tool_test_model_endpoint(provider_id: &str, model_id: &str) -> Value {
    if provider_id.is_empty() || model_id.is_empty() {
        return json!({ "error": "provider_id 和 model_id 不能为空" });
    }

    // 从注册表获取 base_url
    let path = super::get_data_dir().join("model_providers.json");
    let registry: Value = match fs::read_to_string(&path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
    {
        Some(r) => r,
        None => return json!({ "error": "无法读取模型注册表" }),
    };

    let config = super::read_config();
    let api_keys = config.get("apiKeys").cloned().unwrap_or(json!({}));
    let api_key = match api_keys.get(provider_id).and_then(|v| v.as_str()) {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => {
            return json!({ "success": false, "model_id": model_id, "error": format!("未配置 {} 的 API Key", provider_id) })
        }
    };

    let base_url = if let Some(providers) = registry.get("providers").and_then(|v| v.as_array()) {
        providers
            .iter()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(provider_id))
            .map(|p| {
                let base = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
                // Check variant
                let variant_key = format!("providerVariant_{}", provider_id);
                if let Some(variant) = config.get(&variant_key).and_then(|v| v.as_str()) {
                    if !variant.is_empty() && variant != "default" {
                        if let Some(variants) =
                            p.get("base_url_variants").and_then(|v| v.as_object())
                        {
                            if let Some(url) = variants.get(variant).and_then(|v| v.as_str()) {
                                return url.to_string();
                            }
                        }
                    }
                }
                base.to_string()
            })
            .unwrap_or_default()
    } else {
        return json!({ "success": false, "model_id": model_id, "error": "注册表中无供应商信息" });
    };

    if base_url.is_empty() {
        return json!({ "success": false, "model_id": model_id, "error": "找不到供应商的 base_url" });
    }

    let url = format!("{}/chat/completions", base_url);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return json!({ "success": false, "model_id": model_id, "error": format!("HTTP 客户端错误: {}", e) })
        }
    };

    let body = json!({
        "model": model_id,
        "messages": [{ "role": "user", "content": "Hi" }],
        "max_tokens": 5
    });

    let start = std::time::Instant::now();
    let resp = match client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return json!({ "success": false, "model_id": model_id, "error": format!("请求失败: {}", e) })
        }
    };

    let latency_ms = start.elapsed().as_millis();

    if resp.status().is_success() {
        let data: Value = resp.json().await.unwrap_or(json!({}));
        let preview = data
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(50)
            .collect::<String>();
        json!({
            "success": true,
            "model_id": model_id,
            "latency_ms": latency_ms,
            "response_preview": preview
        })
    } else {
        let status = resp.status().as_u16();
        let text: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        json!({
            "success": false,
            "model_id": model_id,
            "error": format!("HTTP {} - {}", status, text)
        })
    }
}

/// update_model_registry — 更新指定供应商的模型列表
fn tool_update_model_registry(provider_id: &str, models: Value) -> Value {
    if provider_id.is_empty() {
        return json!({ "error": "provider_id 不能为空" });
    }

    let models_arr = match models.as_array() {
        Some(arr) => arr,
        None => return json!({ "error": "models 必须是数组" }),
    };

    // 校验每个模型至少有 id 字段
    for m in models_arr {
        if m.get("id").and_then(|v| v.as_str()).is_none() {
            return json!({ "error": format!("模型缺少 id 字段: {:?}", m) });
        }
    }

    let path = super::get_data_dir().join("model_providers.json");
    let mut registry: Value = match fs::read_to_string(&path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
    {
        Some(r) => r,
        None => return json!({ "error": "无法读取模型注册表" }),
    };

    let mut found = false;
    if let Some(providers) = registry.get_mut("providers").and_then(|v| v.as_array_mut()) {
        for p in providers.iter_mut() {
            if p.get("id").and_then(|v| v.as_str()) == Some(provider_id) {
                p["models"] = models.clone();
                found = true;
                break;
            }
        }
    }

    if !found {
        return json!({ "error": format!("未找到供应商: {}", provider_id) });
    }

    registry["last_updated"] = json!(chrono::Local::now().format("%Y-%m-%d").to_string());

    match serde_json::to_string_pretty(&registry) {
        Ok(data) => match fs::write(&path, data) {
            Ok(_) => json!({
                "ok": true,
                "provider": provider_id,
                "models_count": models_arr.len(),
                "message": format!("已更新 {} 的模型列表，共 {} 个模型", provider_id, models_arr.len())
            }),
            Err(e) => json!({ "error": format!("写入文件失败: {}", e) }),
        },
        Err(e) => json!({ "error": format!("序列化失败: {}", e) }),
    }
}

// ═══════════════════════════════════════════════════════════
// browse_page — 浏览器增强工具（发现式 UX）
// ═══════════════════════════════════════════════════════════

async fn tool_browse_page(app: &tauri::AppHandle, args: &Value) -> Value {
    use tauri::Manager;

    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
    if url.is_empty() {
        return json!({ "error": "请提供要访问的网页 URL" });
    }

    // 发现式 UX：浏览器未启用时，返回特殊状态触发前端确认
    if !super::browser::is_browser_enabled() {
        let detected = super::browser::detect_browser();
        return json!({
            "action_required": "browser_enable",
            "message": "此网页需要浏览器增强才能完整加载。请在聊天窗口中确认启用。",
            "browser_detected": detected.is_some(),
            "browser_path": detected.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            "original_url": url
        });
    }

    // 浏览器已启用，执行浏览
    let wait = args
        .get("wait_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(3);
    let click = args.get("click_selector").and_then(|v| v.as_str());
    let extract = args
        .get("extract")
        .and_then(|v| v.as_str())
        .unwrap_or("text");

    let browser_state = app.state::<std::sync::Arc<super::browser::BrowserState>>();

    match super::browser::browse_page(&browser_state, url, wait, click, extract).await {
        Ok(content) => json!({
            "ok": true,
            "url": url,
            "extract_mode": extract,
            "content": content
        }),
        Err(e) => json!({
            "error": format!("浏览器访问失败: {}", e),
            "url": url,
            "suggestion": "可以尝试用 fetch_url 作为降级方案"
        }),
    }
}

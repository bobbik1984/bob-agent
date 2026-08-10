use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RouteMode {
    Direct,
    Deep,
    Advanced,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RouteTaskKind {
    Answer,
    Action,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RouteRisk {
    R0,
    R1,
    R2,
    R3,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RouteDuration {
    Instant,
    Session,
    Persistent,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteSource {
    Override,
    Deterministic,
    Clerk,
    ConservativeFallback,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteDecision {
    pub mode: RouteMode,
    pub task_kind: RouteTaskKind,
    pub confidence: f32,
    pub risk: RouteRisk,
    pub duration: RouteDuration,
    pub source: RouteSource,
    pub reason_codes: Vec<String>,
    pub requires_project_state: bool,
    pub semantic_fallback_recommended: bool,
}

impl std::fmt::Display for RouteDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:?}:{:?} confidence={:.2} risk={:?} source={:?} reasons=[{}]",
            self.mode,
            self.task_kind,
            self.confidence,
            self.risk,
            self.source,
            self.reason_codes.join(",")
        )
    }
}

impl RouteDecision {
    pub(crate) fn tool_intent(&self) -> &'static str {
        match (self.mode, self.task_kind) {
            (_, RouteTaskKind::Answer) => "answer",
            (RouteMode::Direct, RouteTaskKind::Action) => "quick",
            (RouteMode::Deep | RouteMode::Advanced, RouteTaskKind::Action) => "planned",
        }
    }

    pub(crate) fn tool_budget(&self) -> usize {
        match (self.mode, self.task_kind) {
            (RouteMode::Direct, RouteTaskKind::Answer) => 3,
            (RouteMode::Deep | RouteMode::Advanced, RouteTaskKind::Answer) => 8,
            (RouteMode::Direct, RouteTaskKind::Action) => 15,
            (RouteMode::Deep | RouteMode::Advanced, RouteTaskKind::Action) => 30,
        }
    }

    pub(crate) fn system_instruction(&self) -> &'static str {
        match (self.mode, self.task_kind) {
            (RouteMode::Direct, RouteTaskKind::Answer) => {
                "\n## Processing route: Direct answer\nAnswer directly. Read-only tools may be used when needed. Do not perform writes or side effects.\n"
            }
            (RouteMode::Direct, RouteTaskKind::Action) => {
                "\n## Processing route: Direct action\nComplete only the requested single-step action. Prefer reversible operations, follow every permission check, and report exactly what changed.\n"
            }
            (RouteMode::Deep, RouteTaskKind::Answer) => {
                "\n## Processing route: Deep analysis\nUse a bounded research and reasoning process. Verify important claims with read-only tools, then provide a concise conclusion. Do not perform writes or side effects.\n"
            }
            (RouteMode::Deep, RouteTaskKind::Action) => {
                "\n## Processing route: Deep task\nThis is a bounded multi-step task for the current session. State a short plan, execute and verify each material step, and never bypass permission checks.\n"
            }
            (RouteMode::Advanced, RouteTaskKind::Answer) => {
                "\n## Processing route: Advanced work detected\nThis request needs persistent project state or cross-time progress. Phase 5 persistent runtime is not active yet. Perform only a bounded kickoff or analysis, identify the durable next state, and never claim the long-running goal is complete. Use read-only tools only.\n"
            }
            (RouteMode::Advanced, RouteTaskKind::Action) => {
                "\n## Processing route: Advanced work detected\nThis request needs persistent project state, stages, dependencies, recovery, or cross-time progress. Phase 5 persistent runtime is not active yet. Perform only a safe bounded kickoff, preserve explicit next steps and blockers, and never claim the long-running goal is complete. Do not invoke the legacy Goal Loop automatically and never bypass permission checks.\n"
            }
        }
    }
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| text.contains(pattern))
}

fn matching_codes(text: &str, patterns: &[(&str, &str)]) -> Vec<String> {
    patterns
        .iter()
        .filter(|(pattern, _)| text.contains(pattern))
        .map(|(_, code)| (*code).to_string())
        .collect()
}

fn decision(
    mode: RouteMode,
    task_kind: RouteTaskKind,
    confidence: f32,
    risk: RouteRisk,
    source: RouteSource,
    mut reason_codes: Vec<String>,
    semantic_fallback_recommended: bool,
) -> RouteDecision {
    reason_codes.sort();
    reason_codes.dedup();
    RouteDecision {
        mode,
        task_kind,
        confidence: confidence.clamp(0.0, 1.0),
        risk,
        duration: match mode {
            RouteMode::Direct => RouteDuration::Instant,
            RouteMode::Deep => RouteDuration::Session,
            RouteMode::Advanced => RouteDuration::Persistent,
        },
        source,
        reason_codes,
        requires_project_state: mode == RouteMode::Advanced,
        semantic_fallback_recommended,
    }
}

pub(crate) fn last_user_text(messages: &[Value]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|message| {
            let content = message.get("content")?;
            if let Some(text) = content.as_str() {
                return Some(text.to_string());
            }
            content.as_array().map(|items| {
                items
                    .iter()
                    .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
        .unwrap_or_default()
}

fn has_image(messages: &[Value]) -> bool {
    messages.iter().rev().take(2).any(|message| {
        message
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.get("type").and_then(Value::as_str) == Some("image_url"))
            })
    })
}

pub(crate) fn route_messages(messages: &[Value], user_mode: &str) -> RouteDecision {
    let text = last_user_text(messages);
    route_text(&text, has_image(messages), user_mode)
}

pub(crate) fn route_text(text: &str, has_image_input: bool, user_mode: &str) -> RouteDecision {
    let normalized = text.trim().to_lowercase();

    match user_mode {
        "insight" | "direct" => {
            return decision(
                RouteMode::Direct,
                RouteTaskKind::Answer,
                1.0,
                RouteRisk::R0,
                RouteSource::Override,
                vec!["override_read_only".to_string()],
                false,
            );
        }
        "yolo" | "deep" => {
            return decision(
                RouteMode::Deep,
                RouteTaskKind::Action,
                1.0,
                risk_for_action(&normalized),
                RouteSource::Override,
                vec!["override_action".to_string()],
                false,
            );
        }
        "goal" | "advanced" => {
            return decision(
                RouteMode::Advanced,
                RouteTaskKind::Action,
                1.0,
                risk_for_action(&normalized),
                RouteSource::Override,
                vec!["override_goal_prototype".to_string()],
                false,
            );
        }
        _ => {}
    }

    let has_attachment = has_image_input
        || contains_any(
            &normalized,
            &["附件已就绪", "attachment ready", "[附件", "[file:"],
        );

    let schedule_patterns = [
        "提醒我",
        "添加日程",
        "加入日历",
        "创建日程",
        "设个提醒",
        "记到日历",
        "安排会议",
        "安排一个会议",
        "创建待办",
        "新增待办",
        "列为待办",
        "remind me",
        "add to my calendar",
        "schedule a reminder",
        "schedule a meeting",
        "create a todo",
    ];
    let monitoring_patterns = [
        "持续监控",
        "持续观察",
        "每天检查",
        "每周检查",
        "发现异常就",
        "定期检查并",
        "keep monitoring",
        "monitor until",
        "check every day and",
        "check every week and",
    ];
    let is_schedule = contains_any(&normalized, &schedule_patterns)
        && !contains_any(&normalized, &monitoring_patterns);

    if is_schedule {
        return decision(
            RouteMode::Direct,
            RouteTaskKind::Action,
            0.94,
            RouteRisk::R1,
            RouteSource::Deterministic,
            vec!["single_step_schedule".to_string()],
            false,
        );
    }

    let persistent_patterns = [
        ("持续跟进", "persistent_follow_up"),
        ("持续推进", "persistent_follow_up"),
        ("一直跟进", "persistent_follow_up"),
        ("长期跟进", "persistent_follow_up"),
        ("直到完成", "persistent_until_done"),
        ("直到解决", "persistent_until_done"),
        ("不要停", "persistent_until_done"),
        ("一路到底", "persistent_until_done"),
        ("接下来几天", "cross_time"),
        ("未来几天", "cross_time"),
        ("未来几周", "cross_time"),
        ("跨天", "cross_time"),
        ("跨周", "cross_time"),
        ("中断后继续", "recovery_required"),
        ("恢复执行", "recovery_required"),
        ("下次继续", "recovery_required"),
        ("跨会话", "recovery_required"),
        ("分阶段推进", "stage_dependency"),
        ("多个阶段", "stage_dependency"),
        ("依赖前序", "stage_dependency"),
        ("持续监控", "persistent_monitoring"),
        ("每天检查", "persistent_monitoring"),
        ("每周检查", "persistent_monitoring"),
        ("keep working until", "persistent_until_done"),
        ("continue until", "persistent_until_done"),
        ("keep following up", "persistent_follow_up"),
        ("over the next few days", "cross_time"),
        ("over the next few weeks", "cross_time"),
        ("across sessions", "recovery_required"),
        ("resume after", "recovery_required"),
        ("long-running", "cross_time"),
        ("in multiple stages", "stage_dependency"),
        ("depends on the previous", "stage_dependency"),
        ("keep monitoring", "persistent_monitoring"),
        ("monitor until", "persistent_monitoring"),
    ];
    let mut persistent_reasons = matching_codes(&normalized, &persistent_patterns);
    if !persistent_reasons.is_empty() {
        if has_attachment {
            persistent_reasons.push("has_attachment".to_string());
        }
        let task_kind = if explicit_action(&normalized) {
            RouteTaskKind::Action
        } else {
            RouteTaskKind::Answer
        };
        let risk = if task_kind == RouteTaskKind::Action {
            risk_for_action(&normalized)
        } else {
            RouteRisk::R0
        };
        return decision(
            RouteMode::Advanced,
            task_kind,
            0.92,
            risk,
            RouteSource::Deterministic,
            persistent_reasons,
            false,
        );
    }

    let action = explicit_action(&normalized);
    let deep_patterns = [
        ("然后", "multi_step"),
        ("接着", "multi_step"),
        ("之后再", "multi_step"),
        ("逐个", "batch_scope"),
        ("批量", "batch_scope"),
        ("所有文件", "batch_scope"),
        ("所有 pdf", "batch_scope"),
        ("全面分析", "complex_analysis"),
        ("深入分析", "complex_analysis"),
        ("完整分析", "complex_analysis"),
        ("仔细审查", "complex_analysis"),
        ("全面审查", "complex_analysis"),
        ("权衡", "complex_analysis"),
        ("对比并", "complex_analysis"),
        ("比较并", "complex_analysis"),
        ("研究并", "complex_analysis"),
        ("分析并", "complex_analysis"),
        ("验证", "verification_required"),
        ("核对", "verification_required"),
        ("and then", "multi_step"),
        ("step by step", "multi_step"),
        ("for each", "batch_scope"),
        ("all files", "batch_scope"),
        ("all pdf", "batch_scope"),
        ("comprehensive analysis", "complex_analysis"),
        ("deep analysis", "complex_analysis"),
        ("compare and", "complex_analysis"),
        ("research and", "complex_analysis"),
        ("trade-off", "complex_analysis"),
        ("verify", "verification_required"),
        ("validate", "verification_required"),
    ];
    let mut deep_reasons = matching_codes(&normalized, &deep_patterns);
    let conjunction_count = ["然后", "接着", "并且", "同时", " and ", " then "]
        .iter()
        .filter(|pattern| normalized.contains(**pattern))
        .count();
    if conjunction_count >= 2 {
        deep_reasons.push("multi_step".to_string());
    }
    if has_attachment
        && contains_any(
            &normalized,
            &[
                "分析", "整理", "比较", "审查", "analyze", "review", "organize", "compare",
            ],
        )
    {
        deep_reasons.push("has_attachment".to_string());
    }

    if !deep_reasons.is_empty() {
        let task_kind = if action {
            RouteTaskKind::Action
        } else {
            RouteTaskKind::Answer
        };
        let risk = if task_kind == RouteTaskKind::Action {
            risk_for_action(&normalized)
        } else {
            RouteRisk::R0
        };
        return decision(
            RouteMode::Deep,
            task_kind,
            0.86,
            risk,
            RouteSource::Deterministic,
            deep_reasons,
            false,
        );
    }

    if action {
        return decision(
            RouteMode::Direct,
            RouteTaskKind::Action,
            0.82,
            risk_for_action(&normalized),
            RouteSource::Deterministic,
            vec!["explicit_action".to_string()],
            false,
        );
    }

    let ambiguous_patterns = [
        "处理一下",
        "弄一下",
        "看着办",
        "帮我看看这个",
        "看看这个怎么办",
        "handle this",
        "take care of this",
        "do something with this",
        "figure this out",
    ];
    let ambiguous = contains_any(&normalized, &ambiguous_patterns)
        || (has_attachment && normalized.chars().count() < 24);
    if ambiguous {
        let mut reasons = vec!["ambiguous_semantics".to_string()];
        if has_attachment {
            reasons.push("has_attachment".to_string());
        }
        return decision(
            RouteMode::Direct,
            RouteTaskKind::Answer,
            0.45,
            RouteRisk::Unknown,
            RouteSource::Deterministic,
            reasons,
            true,
        );
    }

    let reason = if contains_any(
        &normalized,
        &[
            "什么",
            "为什么",
            "怎么理解",
            "解释",
            "天气",
            "翻译",
            "总结",
            "what",
            "why",
            "explain",
            "weather",
            "translate",
            "summarize",
        ],
    ) {
        "question_or_lookup"
    } else {
        "conservative_direct"
    };
    decision(
        RouteMode::Direct,
        RouteTaskKind::Answer,
        if normalized.is_empty() { 0.35 } else { 0.68 },
        RouteRisk::R0,
        RouteSource::Deterministic,
        vec![reason.to_string()],
        false,
    )
}

fn explicit_action(text: &str) -> bool {
    contains_any(
        text,
        &[
            "创建文件",
            "创建文件夹",
            "新建文件",
            "新建文件夹",
            "写入文件",
            "保存到",
            "导出为",
            "生成报告",
            "生成文档",
            "写一份报告",
            "移动文件",
            "复制文件",
            "删除文件",
            "删除文件夹",
            "重命名",
            "发送邮件",
            "发邮件",
            "发布到",
            "下载安装",
            "安装",
            "更新这个文件",
            "修改这个文件",
            "执行命令",
            "运行命令",
            "提交代码",
            "推送到",
            "记录为待办",
            "加入待办",
            "记一下",
            "收藏这篇",
            "收藏一下",
            "保存这篇",
            "整理进知识库",
            "加入知识库",
            "打开文件",
            "下载文件",
            "转换文件",
            "请执行",
            "create a file",
            "create a folder",
            "write to the file",
            "save to",
            "export as",
            "generate a report",
            "generate a document",
            "move the file",
            "copy the file",
            "delete the file",
            "rename the file",
            "send an email",
            "publish to",
            "install ",
            "update this file",
            "modify this file",
            "run the command",
            "commit the code",
            "push to",
            "add a todo",
            "note this down",
            "save this article",
            "add this to the knowledge base",
            "open the file",
            "download the file",
            "convert the file",
            "please execute",
            "execute the",
        ],
    )
}

fn risk_for_action(text: &str) -> RouteRisk {
    if contains_any(
        text,
        &[
            "格式化",
            "永久删除",
            "清空磁盘",
            "转账",
            "付款",
            "购买",
            "format the drive",
            "permanently delete",
            "transfer money",
            "make a payment",
            "purchase",
        ],
    ) {
        RouteRisk::R3
    } else if contains_any(
        text,
        &[
            "删除",
            "发送邮件",
            "发邮件",
            "发布",
            "推送到",
            "安装",
            "delete",
            "send an email",
            "publish",
            "push to",
            "install",
        ],
    ) {
        RouteRisk::R2
    } else {
        RouteRisk::R1
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClerkRoute {
    mode: RouteMode,
    task_kind: RouteTaskKind,
    confidence: Option<f32>,
}

pub(crate) fn clerk_prompt(text: &str, baseline: &RouteDecision) -> String {
    format!(
        "Classify only processing complexity. Return one JSON object and no markdown: \
{{\"mode\":\"direct|deep|advanced\",\"taskKind\":\"answer|action\",\"confidence\":0.0}}. \
Direct is one answer or one-step action. Deep is bounded multi-step work in this session. \
Advanced requires cross-time state, stages, dependencies, recovery, or persistent follow-up. \
A recurring calendar reminder alone is Direct. Text length alone is not complexity. \
Never infer permission from ambiguity. Baseline={:?}/{:?}. User text: {}",
        baseline.mode, baseline.task_kind, text
    )
}

pub(crate) fn apply_clerk_json(baseline: &RouteDecision, raw: &str) -> Option<RouteDecision> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    let parsed: ClerkRoute = serde_json::from_str(&raw[start..=end]).ok()?;
    let task_kind = if baseline.task_kind == RouteTaskKind::Action {
        parsed.task_kind
    } else {
        RouteTaskKind::Answer
    };
    let risk = if task_kind == RouteTaskKind::Action {
        baseline.risk
    } else {
        RouteRisk::R0
    };
    Some(decision(
        parsed.mode,
        task_kind,
        parsed.confidence.unwrap_or(0.7).clamp(0.55, 0.9),
        risk,
        RouteSource::Clerk,
        vec!["clerk_refinement".to_string()],
        false,
    ))
}

pub(crate) fn conservative_fallback(mut baseline: RouteDecision) -> RouteDecision {
    baseline.source = RouteSource::ConservativeFallback;
    baseline.semantic_fallback_recommended = false;
    if !baseline
        .reason_codes
        .iter()
        .any(|reason| reason == "clerk_unavailable")
    {
        baseline.reason_codes.push("clerk_unavailable".to_string());
    }
    baseline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ReplayCase {
        name: String,
        text: String,
        expected_mode: RouteMode,
        expected_task_kind: RouteTaskKind,
    }

    #[test]
    fn bilingual_replay_set_matches_expected_routes() {
        let cases: Vec<ReplayCase> = serde_json::from_str(include_str!(
            "../../tests/fixtures/router/complexity_router_replay.json"
        ))
        .expect("valid replay fixture");
        assert!(cases.len() >= 24);
        for case in cases {
            let actual = route_text(&case.text, false, "auto");
            assert_eq!(actual.mode, case.expected_mode, "case: {}", case.name);
            assert_eq!(
                actual.task_kind, case.expected_task_kind,
                "case: {}",
                case.name
            );
        }
    }

    #[test]
    fn user_overrides_are_explicit_and_do_not_change_policy() {
        let direct = route_text("删除文件", false, "insight");
        assert_eq!(direct.mode, RouteMode::Direct);
        assert_eq!(direct.task_kind, RouteTaskKind::Answer);
        assert_eq!(direct.risk, RouteRisk::R0);

        let deep = route_text("删除文件", false, "yolo");
        assert_eq!(deep.mode, RouteMode::Deep);
        assert_eq!(deep.risk, RouteRisk::R2);

        let advanced = route_text("继续推进", false, "goal");
        assert_eq!(advanced.mode, RouteMode::Advanced);
        assert!(advanced.requires_project_state);
    }

    #[test]
    fn ambiguous_attachment_recommends_clerk_but_stays_read_only() {
        let route = route_text("帮我看看这个", true, "auto");
        assert_eq!(route.mode, RouteMode::Direct);
        assert_eq!(route.task_kind, RouteTaskKind::Answer);
        assert!(route.semantic_fallback_recommended);
        assert_eq!(route.risk, RouteRisk::Unknown);
    }

    #[test]
    fn clerk_cannot_grant_action_without_deterministic_action_signal() {
        let baseline = route_text("帮我看看这个", true, "auto");
        let refined = apply_clerk_json(
            &baseline,
            r#"```json
            {"mode":"deep","taskKind":"action","confidence":0.99}
            ```"#,
        )
        .expect("valid clerk output");
        assert_eq!(refined.mode, RouteMode::Deep);
        assert_eq!(refined.task_kind, RouteTaskKind::Answer);
        assert_eq!(refined.confidence, 0.9);
        assert_eq!(refined.source, RouteSource::Clerk);
    }

    #[test]
    fn invalid_clerk_output_uses_conservative_fallback() {
        let baseline = route_text("处理一下", false, "auto");
        assert!(apply_clerk_json(&baseline, "not json").is_none());
        let fallback = conservative_fallback(baseline);
        assert_eq!(fallback.mode, RouteMode::Direct);
        assert_eq!(fallback.task_kind, RouteTaskKind::Answer);
        assert_eq!(fallback.source, RouteSource::ConservativeFallback);
    }
}

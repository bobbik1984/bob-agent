use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Weekday};
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::capture::CaptureEnvelope;
use crate::db::DbState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RouteIntent {
    QuickNote,
    Todo,
    Event,
    Source,
    Note,
    WorkTask,
    Decision,
    Artifact,
    Meeting,
    Change,
    Commitment,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDecision {
    pub intent: RouteIntent,
    pub title: String,
    pub date: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub confidence: f32,
    pub needs_clarification: bool,
    pub reason_codes: Vec<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_hint: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

pub fn init_capture_router_tables(conn: &Connection) {
    let _ = conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS capture_enrichment (
            capture_id TEXT PRIMARY KEY,
            route_json TEXT,
            stage TEXT NOT NULL DEFAULT 'pending_model',
            last_error TEXT,
            model_attempts INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_capture_enrichment_stage
            ON capture_enrichment(stage, updated_at);
        ",
    );
}

fn weekday_from_text(text: &str) -> Option<Weekday> {
    let pairs = [
        (["周一", "星期一", "礼拜一"], Weekday::Mon),
        (["周二", "星期二", "礼拜二"], Weekday::Tue),
        (["周三", "星期三", "礼拜三"], Weekday::Wed),
        (["周四", "星期四", "礼拜四"], Weekday::Thu),
        (["周五", "星期五", "礼拜五"], Weekday::Fri),
        (["周六", "星期六", "礼拜六"], Weekday::Sat),
        (["周日", "星期日", "礼拜日"], Weekday::Sun),
    ];
    pairs
        .iter()
        .find(|(terms, _)| terms.iter().any(|term| text.contains(term)))
        .map(|(_, weekday)| *weekday)
}

fn parse_chinese_number(value: &str) -> Option<i64> {
    if let Ok(number) = value.parse::<i64>() {
        return Some(number);
    }
    match value {
        "一" => Some(1),
        "二" | "两" => Some(2),
        "三" => Some(3),
        "四" => Some(4),
        "五" => Some(5),
        "六" => Some(6),
        "七" => Some(7),
        "八" => Some(8),
        "九" => Some(9),
        "十" => Some(10),
        _ => None,
    }
}

fn parse_date(text: &str, today: NaiveDate) -> (Option<NaiveDate>, Vec<String>, bool) {
    if text.contains("大后天") {
        return (
            Some(today + Duration::days(3)),
            vec!["relative_day".into()],
            false,
        );
    }
    if text.contains("后天") {
        return (
            Some(today + Duration::days(2)),
            vec!["relative_day".into()],
            false,
        );
    }
    if text.contains("明天") || text.to_ascii_lowercase().contains("tomorrow") {
        return (
            Some(today + Duration::days(1)),
            vec!["relative_day".into()],
            false,
        );
    }
    if text.contains("今天") || text.to_ascii_lowercase().contains("today") {
        return (Some(today), vec!["relative_day".into()], false);
    }

    if let Ok(re) = Regex::new(r"(?P<n>\d+|[一二两三四五六七八九十])\s*天(?:以)?后") {
        if let Some(caps) = re.captures(text) {
            if let Some(days) = caps
                .name("n")
                .and_then(|m| parse_chinese_number(m.as_str()))
            {
                return (
                    Some(today + Duration::days(days)),
                    vec!["day_offset".into()],
                    false,
                );
            }
        }
    }

    if let Ok(re) = Regex::new(r"(?:(?P<y>\d{4})[年/-])?(?P<m>\d{1,2})[月/-](?P<d>\d{1,2})日?") {
        if let Some(caps) = re.captures(text) {
            let year = caps
                .name("y")
                .and_then(|m| m.as_str().parse::<i32>().ok())
                .unwrap_or(today.year());
            let month = caps.name("m").and_then(|m| m.as_str().parse::<u32>().ok());
            let day = caps.name("d").and_then(|m| m.as_str().parse::<u32>().ok());
            if let (Some(month), Some(day)) = (month, day) {
                if let Some(mut date) = NaiveDate::from_ymd_opt(year, month, day) {
                    if caps.name("y").is_none() && date < today {
                        if let Some(next_year) = NaiveDate::from_ymd_opt(year + 1, month, day) {
                            date = next_year;
                        }
                    }
                    return (Some(date), vec!["calendar_date".into()], false);
                }
            }
            return (None, vec!["invalid_calendar_date".into()], true);
        }
    }

    if let Some(target) = weekday_from_text(text) {
        let current = i64::from(today.weekday().num_days_from_monday());
        let wanted = i64::from(target.num_days_from_monday());
        let mut delta = if text.contains("下下周") {
            14 - current + wanted
        } else if text.contains("下周") || text.contains("下星期") {
            7 - current + wanted
        } else {
            let nearest = (wanted - current).rem_euclid(7);
            if nearest == 0 {
                7
            } else {
                nearest
            }
        };
        // Defensive guard for malformed combinations such as repeated prefixes.
        delta = delta.max(1);
        return (
            Some(today + Duration::days(delta)),
            vec!["weekday".into()],
            false,
        );
    }

    let fuzzy = ["过几天", "这几天", "近期", "有空", "找个时间", "周末"]
        .iter()
        .any(|term| text.contains(term));
    (
        None,
        vec![if fuzzy { "fuzzy_date" } else { "no_date" }.into()],
        fuzzy,
    )
}

fn parse_time(text: &str) -> (Option<NaiveTime>, Vec<String>, bool) {
    if let Ok(re) = Regex::new(r"(?P<h>\d{1,2})[:：](?P<m>\d{2})") {
        if let Some(caps) = re.captures(text) {
            let hour = caps.name("h").and_then(|m| m.as_str().parse::<u32>().ok());
            let minute = caps.name("m").and_then(|m| m.as_str().parse::<u32>().ok());
            return match (hour, minute) {
                (Some(hour), Some(minute)) => (
                    NaiveTime::from_hms_opt(hour, minute, 0),
                    vec!["clock_time".into()],
                    hour > 23 || minute > 59,
                ),
                _ => (None, vec!["invalid_clock_time".into()], true),
            };
        }
    }
    if let Ok(re) = Regex::new(r"(?P<h>\d{1,2}|[一二两三四五六七八九十])点(?P<half>半)?")
    {
        if let Some(caps) = re.captures(text) {
            if let Some(mut hour) = caps
                .name("h")
                .and_then(|m| parse_chinese_number(m.as_str()))
            {
                if (text.contains("下午") || text.contains("晚上")) && hour < 12 {
                    hour += 12;
                }
                if text.contains("中午") && hour < 11 {
                    hour += 12;
                }
                let minute = if caps.name("half").is_some() { 30 } else { 0 };
                return (
                    NaiveTime::from_hms_opt(hour as u32, minute, 0),
                    vec!["spoken_time".into()],
                    hour > 23,
                );
            }
        }
    }
    let has_period = ["上午", "下午", "晚上", "早上", "中午"]
        .iter()
        .any(|term| text.contains(term));
    (
        None,
        vec![if has_period {
            "fuzzy_period"
        } else {
            "no_time"
        }
        .into()],
        has_period,
    )
}

fn clean_title(text: &str) -> String {
    let mut title = text.trim().to_string();
    for prefix in [
        "提醒我",
        "帮我记得",
        "添加待办",
        "新建待办",
        "安排",
        "日程：",
        "待办：",
    ] {
        title = title.trim_start_matches(prefix).trim().to_string();
    }
    title.chars().take(100).collect()
}

fn explicit_project_reference(text: &str) -> (Option<String>, Option<String>) {
    if let Ok(id_pattern) = Regex::new(r"\b(project_[A-Za-z0-9_-]+)\b") {
        if let Some(value) = id_pattern
            .captures(text)
            .and_then(|captures| captures.get(1))
        {
            return (Some(value.as_str().to_string()), None);
        }
    }
    if let Ok(name_pattern) =
        Regex::new(r"(?:在|归入|属于)\s*([^，,:：。]{1,80}?)\s*项目(?:中|里|下)?")
    {
        if let Some(value) = name_pattern
            .captures(text)
            .and_then(|captures| captures.get(1))
        {
            return (None, Some(value.as_str().trim().to_string()));
        }
    }
    (None, None)
}

fn work_route(capture: &CaptureEnvelope, intent: RouteIntent) -> RouteDecision {
    let text = capture.content.as_deref().unwrap_or("").trim();
    let (project_id, project_hint) = explicit_project_reference(text);
    let reason = text
        .split_once("因为")
        .map(|(_, value)| value.trim().to_string())
        .or_else(|| {
            text.to_lowercase()
                .split_once("because")
                .map(|(_, value)| value.trim().to_string())
        });
    RouteDecision {
        intent,
        title: clean_title(text),
        date: None,
        start_time: None,
        end_time: None,
        confidence: 1.0,
        needs_clarification: false,
        reason_codes: vec!["explicit_work_intent".into()],
        project_id,
        project_hint,
        tags: vec![],
        domains: vec![],
        topics: vec![],
        reason,
        owner: None,
        due_at: None,
        metadata: json!({}),
    }
}

pub fn route_locally(capture: &CaptureEnvelope) -> RouteDecision {
    let text = capture.content.as_deref().unwrap_or("").trim();
    let explicit = capture.explicit_intent.as_deref().unwrap_or("");
    if let Some(intent) = match explicit {
        "work_task" => Some(RouteIntent::WorkTask),
        "decision" => Some(RouteIntent::Decision),
        "artifact" => Some(RouteIntent::Artifact),
        "meeting" => Some(RouteIntent::Meeting),
        "change" => Some(RouteIntent::Change),
        "commitment" => Some(RouteIntent::Commitment),
        _ => None,
    } {
        return work_route(capture, intent);
    }
    if explicit == "seed" {
        return RouteDecision {
            intent: RouteIntent::QuickNote,
            title: clean_title(text),
            date: None,
            start_time: None,
            end_time: None,
            confidence: 1.0,
            needs_clarification: false,
            reason_codes: vec!["explicit_seed".into()],
            project_id: None,
            project_hint: None,
            tags: vec![],
            domains: vec![],
            topics: vec![],
            reason: None,
            owner: None,
            due_at: None,
            metadata: json!({}),
        };
    }
    if capture.source_url.is_some() || matches!(explicit, "source" | "knowledge") {
        let (project_id, project_hint) = explicit_project_reference(text);
        return RouteDecision {
            intent: RouteIntent::Source,
            title: clean_title(text),
            date: None,
            start_time: None,
            end_time: None,
            confidence: 0.98,
            needs_clarification: false,
            reason_codes: vec!["source_detected".into()],
            project_id,
            project_hint,
            tags: vec![],
            domains: vec![],
            topics: vec![],
            reason: None,
            owner: None,
            due_at: None,
            metadata: json!({}),
        };
    }

    let lower = text.to_lowercase();
    let event_signal = ["开会", "会议", "见面", "约见", "日程", "参加"]
        .iter()
        .any(|term| lower.contains(term));
    let todo_signal = ["待办", "任务", "提醒我", "记得", "todo"]
        .iter()
        .any(|term| lower.contains(term));
    let (date, mut reasons, date_ambiguous) = parse_date(text, Local::now().date_naive());
    let (time, time_reasons, time_ambiguous) = parse_time(text);
    reasons.extend(time_reasons);

    let intent = if event_signal {
        RouteIntent::Event
    } else if todo_signal {
        RouteIntent::Todo
    } else {
        RouteIntent::Pending
    };
    let requires_exact_time = intent == RouteIntent::Event;
    let missing_required = date.is_none() || (requires_exact_time && time.is_none());
    let needs_clarification = date_ambiguous || time_ambiguous || missing_required;
    let confidence = if intent == RouteIntent::Pending {
        0.35
    } else if needs_clarification {
        0.62
    } else {
        0.94
    };
    let (project_id, project_hint) = explicit_project_reference(text);
    RouteDecision {
        intent,
        title: clean_title(text),
        date: date.map(|value| value.format("%Y-%m-%d").to_string()),
        start_time: time.map(|value| value.format("%H:%M").to_string()),
        end_time: None,
        confidence,
        needs_clarification,
        reason_codes: reasons,
        project_id,
        project_hint,
        tags: vec![],
        domains: vec![],
        topics: vec![],
        reason: None,
        owner: None,
        due_at: None,
        metadata: json!({}),
    }
}

fn project_proposal(
    capture: &CaptureEnvelope,
    route: &RouteDecision,
    external_kind: Option<&str>,
    external_id: Option<String>,
    mut metadata: Value,
) -> Option<crate::work_core::project_links::ProjectLinkProposal> {
    use crate::work_core::project_links::{ProjectLinkIntent, ProjectLinkProposal};
    let intent = match route.intent {
        RouteIntent::WorkTask => ProjectLinkIntent::WorkTask,
        RouteIntent::Decision => ProjectLinkIntent::Decision,
        RouteIntent::Todo => ProjectLinkIntent::Todo,
        RouteIntent::Event => ProjectLinkIntent::Event,
        RouteIntent::Note => ProjectLinkIntent::Note,
        RouteIntent::Source => ProjectLinkIntent::Source,
        RouteIntent::Artifact => ProjectLinkIntent::Artifact,
        RouteIntent::Meeting => ProjectLinkIntent::Meeting,
        RouteIntent::Change => ProjectLinkIntent::Change,
        RouteIntent::Commitment => ProjectLinkIntent::Commitment,
        _ => return None,
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("captureId".into(), json!(capture.capture_id));
    }
    Some(ProjectLinkProposal {
        intent,
        title: route.title.clone(),
        project_id: route.project_id.clone(),
        project_hint: route.project_hint.clone(),
        description: capture.content.clone(),
        reason: route.reason.clone(),
        owner: route.owner.clone(),
        due_at: route.due_at.clone(),
        external_kind: external_kind.map(str::to_string),
        external_id,
        metadata,
        confidence: route.confidence as f64,
        reason_codes: route.reason_codes.clone(),
    })
}

fn validate_route(route: &RouteDecision) -> Result<(), String> {
    if route.title.trim().is_empty() {
        return Err("缺少事项标题".into());
    }
    if let Some(date) = &route.date {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "日期格式无效".to_string())?;
    }
    if let Some(time) = &route.start_time {
        NaiveTime::parse_from_str(time, "%H:%M").map_err(|_| "时间格式无效".to_string())?;
    }
    if route.intent == RouteIntent::Event && (route.date.is_none() || route.start_time.is_none()) {
        return Err("日程需要明确日期和时间".into());
    }
    if route.intent == RouteIntent::Todo && route.date.is_none() {
        return Err("待办日期尚不明确".into());
    }
    Ok(())
}

fn save_route(
    conn: &Connection,
    capture_id: &str,
    route: &RouteDecision,
    stage: &str,
) -> Result<(), String> {
    let route_json = serde_json::to_string(route).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO capture_enrichment (capture_id, route_json, stage, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(capture_id) DO UPDATE SET route_json = excluded.route_json, stage = excluded.stage, last_error = NULL, updated_at = excluded.updated_at",
        params![capture_id, route_json, stage, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn commit_action(
    conn: &Connection,
    capture: &CaptureEnvelope,
    route: &RouteDecision,
) -> Result<String, String> {
    validate_route(route)?;
    let event_type = match route.intent {
        RouteIntent::Todo => "todo",
        RouteIntent::Event => "event",
        _ => return Err("该分流结果不是待办或日程".into()),
    };
    let event_id = format!("capture:{}:{}", capture.capture_id, event_type);
    conn.execute(
        "INSERT INTO events (id, title, type, status, date, start_time, end_time, description, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(id) DO NOTHING",
        params![
            &event_id,
            &route.title,
            event_type,
            &route.date,
            &route.start_time,
            &route.end_time,
            capture.content.as_deref().unwrap_or(""),
            crate::now_ms(),
        ],
    )
    .map_err(|e| e.to_string())?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE id = ?1",
            params![&event_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists != 1 {
        return Err("待办或日程提交后未获得数据库回执".into());
    }
    Ok(format!("event:{event_id}"))
}

fn commit_knowledge(
    conn: &Connection,
    capture: &CaptureEnvelope,
    route: &RouteDecision,
) -> Result<crate::knowledge_committer::CommitOutcome, String> {
    save_route(conn, &capture.capture_id, route, "validated")?;
    let mut write_route = route.clone();
    if write_route.intent == RouteIntent::Source {
        write_route.project_id = None;
        write_route.project_hint = None;
    } else if write_route.intent == RouteIntent::Note && write_route.project_id.is_none() {
        if let Some(proposal) = project_proposal(capture, route, None, None, json!({})) {
            write_route.project_id =
                crate::work_core::project_links::resolve_unique_project_id(conn, &proposal)?;
        }
        write_route.project_hint = None;
    }
    let outcome = crate::knowledge_committer::commit_knowledge_capture(capture, &write_route)?;
    let derived_ref = outcome.relative_path.clone();
    crate::capture::mark_knowledge_committed(conn, capture, std::slice::from_ref(&derived_ref))?;
    conn.execute(
        "UPDATE capture_enrichment SET stage = 'committed', updated_at = ?2 WHERE capture_id = ?1",
        params![&capture.capture_id, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(outcome)
}

pub fn apply_local_route(
    conn: &mut Connection,
    capture: &CaptureEnvelope,
) -> Result<Value, String> {
    let route = route_locally(capture);
    match route.intent {
        RouteIntent::Todo | RouteIntent::Event if !route.needs_clarification => {
            save_route(conn, &capture.capture_id, &route, "validated")?;
            let derived_ref = commit_action(conn, capture, &route)?;
            crate::capture::mark_routed_committed(conn, capture, &[derived_ref.clone()])?;
            conn.execute(
                "UPDATE capture_enrichment SET stage = 'committed', updated_at = ?2 WHERE capture_id = ?1",
                params![&capture.capture_id, crate::now_ms()],
            )
            .map_err(|e| e.to_string())?;
            if let Some(proposal) = project_proposal(
                capture,
                &route,
                Some("calendar_event"),
                derived_ref.strip_prefix("event:").map(str::to_string),
                json!({ "date": route.date, "startTime": route.start_time, "endTime": route.end_time }),
            ) {
                let _ = crate::work_core::project_links::apply_proposal(conn, capture, proposal)?;
            }
            Ok(json!({ "route": route, "committed": true, "derivedRef": derived_ref }))
        }
        RouteIntent::QuickNote | RouteIntent::Source => {
            let outcome = commit_knowledge(conn, capture, &route)?;
            if let Some(proposal) = project_proposal(
                capture,
                &route,
                Some("knowledge_source"),
                Some(outcome.object_id.clone()),
                json!({ "path": outcome.relative_path }),
            ) {
                let _ = crate::work_core::project_links::apply_proposal(conn, capture, proposal)?;
            }
            Ok(json!({ "route": route, "committed": true, "derivedRef": outcome.relative_path }))
        }
        RouteIntent::WorkTask
        | RouteIntent::Decision
        | RouteIntent::Artifact
        | RouteIntent::Meeting
        | RouteIntent::Change
        | RouteIntent::Commitment => {
            save_route(conn, &capture.capture_id, &route, "validated")?;
            let mut metadata = route.metadata.clone();
            if matches!(route.intent, RouteIntent::Artifact | RouteIntent::Change) {
                if let Some(path) = capture.file_path.as_deref() {
                    let fingerprint =
                        serde_json::to_value(crate::work_core::project_links::fingerprint_file(
                            std::path::Path::new(path),
                        )?)
                        .map_err(|e| e.to_string())?;
                    let object = metadata
                        .as_object_mut()
                        .ok_or_else(|| "Artifact metadata 必须是 JSON object".to_string())?;
                    if let Some(fingerprint) = fingerprint.as_object() {
                        for (key, value) in fingerprint {
                            object.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            let proposal = project_proposal(
                capture,
                &route,
                capture.file_path.as_ref().map(|_| "file"),
                capture.file_path.clone(),
                metadata,
            )
            .ok_or_else(|| "无法构建项目关联建议".to_string())?;
            let outcome = crate::work_core::project_links::apply_proposal(conn, capture, proposal)?;
            let pending = outcome.as_ref().map(|v| v.candidate.status.as_str()) == Some("pending");
            Ok(
                json!({ "route": route, "committed": !pending, "projectAssignmentPending": pending, "outcome": outcome }),
            )
        }
        _ => {
            save_route(conn, &capture.capture_id, &route, "pending_model")?;
            crate::capture::mark_pending_enrichment(conn, capture, &route)?;
            Ok(json!({ "route": route, "committed": false, "pendingEnrichment": true }))
        }
    }
}

fn parse_clerk_route(raw: &str) -> Result<RouteDecision, String> {
    let trimmed = raw.trim();
    let json_text = if trimmed.contains("```json") {
        trimmed
            .split("```json")
            .nth(1)
            .and_then(|part| part.split("```").next())
            .unwrap_or(trimmed)
    } else {
        trimmed
    };
    serde_json::from_str(json_text).map_err(|e| format!("Clerk 路由结果不是有效 JSON: {e}"))
}

async fn clerk_route(capture: &CaptureEnvelope) -> Result<RouteDecision, String> {
    let prompt = format!(
        "将下面输入解析为 JSON。intent 只能是 todo、event、quick_note、note、source、work_task、decision、artifact、meeting、change、commitment、pending。普通提醒和日程继续使用 todo/event；只有明确要求写入某个项目的任务、决策、文件成果、会议结论、变化或承诺时才使用工作意图。不要调用工具，不要猜测缺失日期、项目 ID 或影响关系。date 使用 YYYY-MM-DD，startTime/endTime 使用 HH:MM；projectId 只有输入明确包含 project_ ID 时才能填写；用户只说项目名称时写入 projectHint，由本地索引唯一精确匹配。decision 必须提取 reason，并把完整结构放入 metadata.decisionData：decision、reason、alternatives、rejectedAlternatives（option/reason）、participants、owner、evidence、revisitCondition；未提及的可选字段使用空数组或 null。commitment 必须提取 owner 和 dueAt；meeting 把明确形成的 decision/task/commitment 放入 metadata.items，decision item 使用相同完整字段。change 只有在输入明确引用 Work Object ID 时才写入 metadata.affectedObjectIds；若输入还明确表达关系，可写 metadata.impacts 数组，每项仅包含 objectId、relation（affected_by/contradicts/supersedes）、explanation、evidenceRefs、confidence，不得依靠猜测填写。tags、domains、topics 使用简短数组；信息不足时 needsClarification=true。当前本地日期是 {}。\n\n输入：{}",
        Local::now().date_naive(),
        capture.content.as_deref().unwrap_or("")
    );
    let raw = crate::llm::call_clerk_oneshot(
        "你是离线优先个人助理的结构化意图解析器。只输出符合字段 intent,title,date,startTime,endTime,confidence,needsClarification,reasonCodes,projectId,projectHint,tags,domains,topics,reason,owner,dueAt,metadata 的 JSON。模型只提出候选，项目归属和最终写入由本地确定性代码决定。",
        &prompt,
        400,
    )
    .await
    .ok_or_else(|| "Clerk 当前不可用".to_string())?;
    parse_clerk_route(&raw)
}

async fn process_capture(app: &AppHandle, capture: CaptureEnvelope) -> Result<Value, String> {
    let saved_route = {
        let db = app.state::<DbState>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT route_json FROM capture_enrichment WHERE capture_id = ?1 AND stage IN ('validated','awaiting_pipeline')",
            params![&capture.capture_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    };
    let route = match saved_route {
        Some(value) => serde_json::from_str(&value).map_err(|e| e.to_string())?,
        None => clerk_route(&capture).await?,
    };
    if matches!(
        route.intent,
        RouteIntent::QuickNote | RouteIntent::Note | RouteIntent::Source
    ) {
        let db = app.state::<DbState>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let outcome = match commit_knowledge(&conn, &capture, &route) {
            Ok(value) => value,
            Err(error) if crate::knowledge_committer::is_project_clarification_error(&error) => {
                let mut clarification_route = route.clone();
                clarification_route.needs_clarification = true;
                clarification_route
                    .reason_codes
                    .push("project_match_required".into());
                save_route(
                    &conn,
                    &capture.capture_id,
                    &clarification_route,
                    "needs_clarification",
                )?;
                crate::capture::mark_needs_clarification(&conn, &capture, &clarification_route)?;
                return Ok(json!({
                    "captureId": capture.capture_id,
                    "needsClarification": true,
                    "reason": "project_match_required"
                }));
            }
            Err(error) => return Err(error),
        };
        let derived_ref = outcome.relative_path.clone();
        drop(conn);
        let db = app.state::<DbState>();
        let mut conn = db.0.lock().map_err(|e| e.to_string())?;
        if let Some(proposal) = project_proposal(
            &capture,
            &route,
            Some(if route.intent == RouteIntent::Note {
                "knowledge_note"
            } else {
                "knowledge_source"
            }),
            Some(outcome.object_id.clone()),
            json!({ "path": outcome.relative_path }),
        ) {
            let _ = crate::work_core::project_links::apply_proposal(&mut conn, &capture, proposal)?;
        }
        return Ok(json!({
            "captureId": capture.capture_id,
            "committed": true,
            "intent": route.intent,
            "derivedRef": derived_ref
        }));
    }
    if matches!(
        route.intent,
        RouteIntent::WorkTask
            | RouteIntent::Decision
            | RouteIntent::Artifact
            | RouteIntent::Meeting
            | RouteIntent::Change
            | RouteIntent::Commitment
    ) {
        let db = app.state::<DbState>();
        let mut conn = db.0.lock().map_err(|e| e.to_string())?;
        save_route(&conn, &capture.capture_id, &route, "validated")?;
        let mut metadata = route.metadata.clone();
        if route.intent == RouteIntent::Artifact {
            if let Some(path) = capture.file_path.as_deref() {
                let fingerprint =
                    crate::work_core::project_links::fingerprint_file(std::path::Path::new(path))?;
                metadata = serde_json::to_value(fingerprint).map_err(|e| e.to_string())?;
            }
        }
        let proposal = project_proposal(
            &capture,
            &route,
            capture.file_path.as_ref().map(|_| "file"),
            capture.file_path.clone(),
            metadata,
        )
        .ok_or_else(|| "无法构建项目关联建议".to_string())?;
        let outcome =
            crate::work_core::project_links::apply_proposal(&mut conn, &capture, proposal)?;
        let pending = outcome.as_ref().map(|v| v.candidate.status.as_str()) == Some("pending");
        return Ok(
            json!({ "captureId": capture.capture_id, "committed": !pending, "projectAssignmentPending": pending, "outcome": outcome }),
        );
    }
    if !matches!(route.intent, RouteIntent::Todo | RouteIntent::Event) {
        let db = app.state::<DbState>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        save_route(&conn, &capture.capture_id, &route, "needs_clarification")?;
        crate::capture::mark_needs_clarification(&conn, &capture, &route)?;
        return Ok(json!({ "captureId": capture.capture_id, "needsClarification": true }));
    }
    if route.needs_clarification || validate_route(&route).is_err() {
        let db = app.state::<DbState>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        save_route(&conn, &capture.capture_id, &route, "needs_clarification")?;
        crate::capture::mark_needs_clarification(&conn, &capture, &route)?;
        return Ok(json!({ "captureId": capture.capture_id, "needsClarification": true }));
    }
    let db = app.state::<DbState>();
    let mut conn = db.0.lock().map_err(|e| e.to_string())?;
    save_route(&conn, &capture.capture_id, &route, "validated")?;
    let derived_ref = commit_action(&conn, &capture, &route)?;
    crate::capture::mark_routed_committed(&conn, &capture, &[derived_ref.clone()])?;
    conn.execute(
        "UPDATE capture_enrichment SET stage = 'committed', updated_at = ?2 WHERE capture_id = ?1",
        params![&capture.capture_id, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    if let Some(proposal) = project_proposal(
        &capture,
        &route,
        Some("calendar_event"),
        derived_ref.strip_prefix("event:").map(str::to_string),
        json!({ "date": route.date, "startTime": route.start_time, "endTime": route.end_time }),
    ) {
        let _ = crate::work_core::project_links::apply_proposal(&mut conn, &capture, proposal)?;
    }
    Ok(json!({ "captureId": capture.capture_id, "committed": true, "derivedRef": derived_ref }))
}

pub(crate) async fn process_capture_by_id(
    app: &AppHandle,
    capture_id: &str,
) -> Result<Value, String> {
    let capture = {
        let db = app.state::<DbState>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        crate::capture::get_capture(&conn, capture_id)?
    };
    process_capture(app, capture).await
}

#[tauri::command]
pub async fn capture_process_pending(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Value, String> {
    let captures = {
        let db = app.state::<DbState>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        crate::capture::list_pending_enrichment(&conn, limit.unwrap_or(10).clamp(1, 20))?
    };
    let mut processed = 0;
    let mut committed = 0;
    let mut needs_clarification = 0;
    let mut deferred = 0;
    let mut awaiting_pipeline = 0;
    for capture in captures {
        processed += 1;
        match process_capture(&app, capture.clone()).await {
            Ok(result) if result.get("committed").and_then(Value::as_bool) == Some(true) => {
                committed += 1
            }
            Ok(result) if result.get("pendingPipeline").and_then(Value::as_bool) == Some(true) => {
                awaiting_pipeline += 1
            }
            Ok(_) => needs_clarification += 1,
            Err(error) => {
                deferred += 1;
                let db = app.state::<DbState>();
                let conn = db.0.lock().map_err(|e| e.to_string())?;
                crate::capture::mark_enrichment_retry(&conn, &capture, &error)?;
            }
        }
    }
    Ok(json!({
        "ok": true,
        "processed": processed,
        "committed": committed,
        "needsClarification": needs_clarification,
        "awaitingPipeline": awaiting_pipeline,
        "deferred": deferred
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{build_envelope_for_test, CaptureInput};

    fn capture(text: &str) -> CaptureEnvelope {
        build_envelope_for_test(CaptureInput {
            entry_point: "chat".into(),
            content: Some(text.into()),
            source_url: None,
            file_path: None,
            explicit_intent: None,
            language: Some("zh-CN".into()),
            privacy_scope: None,
            sync_scope: None,
            source_device: Some("test".into()),
            idempotency_key: Some(format!("test:{text}")),
        })
        .unwrap()
    }

    #[test]
    fn explicit_relative_date_uses_local_fast_path() {
        let route = route_locally(&capture("明天下午三点提醒我交报告"));
        assert_eq!(route.intent, RouteIntent::Todo);
        assert_eq!(route.start_time.as_deref(), Some("15:00"));
        assert!(!route.needs_clarification);
        assert!(route.confidence > 0.9);
    }

    #[test]
    fn vague_date_waits_for_enrichment_or_clarification() {
        let route = route_locally(&capture("过几天提醒我看看合同"));
        assert_eq!(route.intent, RouteIntent::Todo);
        assert!(route.needs_clarification);
        assert!(route.date.is_none());
    }

    #[test]
    fn next_weekday_is_calculated_from_next_calendar_week() {
        let tuesday = NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        let (date, _, ambiguous) = parse_date("下周一开会", tuesday);
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 8, 17));
        assert!(!ambiguous);
    }

    #[test]
    fn deterministic_event_commit_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        crate::calendar::init_events_table(&conn);
        init_capture_router_tables(&conn);
        let capture = capture("明天下午三点开会");
        let route = route_locally(&capture);
        let first = commit_action(&conn, &capture, &route).unwrap();
        let second = commit_action(&conn, &capture, &route).unwrap();
        assert_eq!(first, second);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn invalid_clerk_date_cannot_reach_calendar() {
        let route = parse_clerk_route(r#"{"intent":"event","title":"会议","date":"2026-02-30","startTime":"15:00","endTime":null,"confidence":0.9,"needsClarification":false,"reasonCodes":[]}"#).unwrap();
        assert!(validate_route(&route).is_err());
    }
}

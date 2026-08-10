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

pub fn route_locally(capture: &CaptureEnvelope) -> RouteDecision {
    let text = capture.content.as_deref().unwrap_or("").trim();
    let explicit = capture.explicit_intent.as_deref().unwrap_or("");
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
        };
    }
    if capture.source_url.is_some() || matches!(explicit, "source" | "knowledge") {
        return RouteDecision {
            intent: RouteIntent::Source,
            title: clean_title(text),
            date: None,
            start_time: None,
            end_time: None,
            confidence: 0.98,
            needs_clarification: false,
            reason_codes: vec!["source_detected".into()],
            project_id: None,
            project_hint: None,
            tags: vec![],
            domains: vec![],
            topics: vec![],
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
    RouteDecision {
        intent,
        title: clean_title(text),
        date: date.map(|value| value.format("%Y-%m-%d").to_string()),
        start_time: time.map(|value| value.format("%H:%M").to_string()),
        end_time: None,
        confidence,
        needs_clarification,
        reason_codes: reasons,
        project_id: None,
        project_hint: None,
        tags: vec![],
        domains: vec![],
        topics: vec![],
    }
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
) -> Result<String, String> {
    save_route(conn, &capture.capture_id, route, "validated")?;
    let outcome = crate::knowledge_committer::commit_knowledge_capture(capture, route)?;
    let derived_ref = outcome.relative_path;
    crate::capture::mark_knowledge_committed(conn, capture, std::slice::from_ref(&derived_ref))?;
    conn.execute(
        "UPDATE capture_enrichment SET stage = 'committed', updated_at = ?2 WHERE capture_id = ?1",
        params![&capture.capture_id, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(derived_ref)
}

pub fn apply_local_route(conn: &Connection, capture: &CaptureEnvelope) -> Result<Value, String> {
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
            Ok(json!({ "route": route, "committed": true, "derivedRef": derived_ref }))
        }
        RouteIntent::QuickNote | RouteIntent::Source => {
            let derived_ref = commit_knowledge(conn, capture, &route)?;
            Ok(json!({ "route": route, "committed": true, "derivedRef": derived_ref }))
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
        "将下面输入解析为 JSON。intent 只能是 todo、event、quick_note、note、source、pending。不要调用工具，不要猜测缺失日期或项目 ID。date 使用 YYYY-MM-DD，startTime/endTime 使用 HH:MM；projectId 只有输入明确包含 project_ ID 时才能填写；用户只说项目名称时写入 projectHint，由本地索引唯一匹配；tags、domains、topics 使用简短数组；信息不足时 needsClarification=true。当前本地日期是 {}。\n\n输入：{}",
        Local::now().date_naive(),
        capture.content.as_deref().unwrap_or("")
    );
    let raw = crate::llm::call_clerk_oneshot(
        "你是离线优先个人助理的结构化意图解析器。只输出符合字段 intent,title,date,startTime,endTime,confidence,needsClarification,reasonCodes,projectId,projectHint,tags,domains,topics 的 JSON。",
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
        let derived_ref = match commit_knowledge(&conn, &capture, &route) {
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
        return Ok(json!({
            "captureId": capture.capture_id,
            "committed": true,
            "intent": route.intent,
            "derivedRef": derived_ref
        }));
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
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    save_route(&conn, &capture.capture_id, &route, "validated")?;
    let derived_ref = commit_action(&conn, &capture, &route)?;
    crate::capture::mark_routed_committed(&conn, &capture, &[derived_ref.clone()])?;
    conn.execute(
        "UPDATE capture_enrichment SET stage = 'committed', updated_at = ?2 WHERE capture_id = ?1",
        params![&capture.capture_id, crate::now_ms()],
    )
    .map_err(|e| e.to_string())?;
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

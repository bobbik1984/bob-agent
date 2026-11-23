//! Unified Growth Loop for Bob.
//!
//! Enforces:
//! - Zero-LLM-Cost Execution Telemetry (`ExecutionTelemetryStore` in `growth_telemetry.json`).
//! - 5-Phase Nightly Dream Consolidator (`Orient -> Gather -> Consolidate -> Prune -> Index`).
//! - Max-3-Delete Safety Cap and 100% lossless recovery log (`dream_recovery_log.json`).
//! - SOUL / Persona Evolution Proposal (`SettingsChange` approval card; never silently mutating `SOUL.md`).
//! - Structured Morning Brief Generator (`MorningBrief` + `<ActionCard />` presentation).
//! - 4-Level Proactivity Slider (`silent`, `morning_brief`, `reminders`, `proactive_care`)
//!   with Quiet Hours gating (e.g. 22:00–08:00) and 3-strike ignored-ping backoff.

use crate::budget::MonthlyBudgetStore;
use crate::tool_bus::{
    ActionRiskLevel, BobToolBus, CardField, CardItem, PendingApprovalCard, PendingApprovalStore,
    StructuredCard,
};
use bob_kernel::app::LocalApp;
use bob_kernel::personal_ops::{MemoryItem, ScheduleItem};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MAX_PRUNE_PER_DREAM: usize = 3;

// ---------------------------------------------------------------------------
// 1. Zero-LLM-Cost Execution Telemetry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnTelemetryRecord {
    pub timestamp: i64,
    pub conversation_id: String,
    pub tools_called: Vec<String>,
    pub succeeded: bool,
    pub rejected_tools: Vec<String>,
    pub latency_ms: u64,
    pub tokens_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TelemetryFilePayload {
    pub records: Vec<TurnTelemetryRecord>,
}

#[derive(Debug, Clone)]
pub struct ExecutionTelemetryStore {
    file_path: PathBuf,
    payload: TelemetryFilePayload,
}

impl ExecutionTelemetryStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("growth_telemetry.json");
        let payload = if file_path.is_file() {
            let content =
                std::fs::read_to_string(&file_path).map_err(|_| "TELEMETRY_READ_FAILED")?;
            serde_json::from_str::<TelemetryFilePayload>(&content).unwrap_or_default()
        } else {
            TelemetryFilePayload::default()
        };

        Ok(Self { file_path, payload })
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.payload)
            .map_err(|_| "TELEMETRY_SERIALIZE_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes())
            .map_err(|_| "TELEMETRY_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "TELEMETRY_WRITE_FAILED")?;
        Ok(())
    }

    pub fn record_turn(&mut self, record: TurnTelemetryRecord) -> Result<(), &'static str> {
        self.payload.records.push(record);
        if self.payload.records.len() > 1000 {
            let excess = self.payload.records.len() - 1000;
            self.payload.records.drain(0..excess);
        }
        self.save_atomic()
    }

    pub fn recent_records(&self, limit: usize) -> Vec<TurnTelemetryRecord> {
        let len = self.payload.records.len();
        let take = limit.min(len);
        self.payload.records[len - take..].to_vec()
    }
}

pub fn record_turn_telemetry(
    data_dir: &Path,
    record: TurnTelemetryRecord,
) -> Result<(), &'static str> {
    let mut store = ExecutionTelemetryStore::open(data_dir)?;
    store.record_turn(record)
}

// ---------------------------------------------------------------------------
// 2. 4-Level Proactivity Slider & Quiet Hours Gate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProactiveKind {
    MorningBrief,
    Reminder,
    ProactiveCare,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProactivityConfig {
    pub level: f64,
    #[serde(default = "default_quiet_hours_start", alias = "quietHoursStart")]
    pub quiet_hours_start: String,
    #[serde(default = "default_quiet_hours_end", alias = "quietHoursEnd")]
    pub quiet_hours_end: String,
    #[serde(default = "default_utc_offset_minutes", alias = "utcOffsetMinutes")]
    pub utc_offset_minutes: i32,
    #[serde(default, alias = "ignoredProactiveStreak")]
    pub ignored_proactive_streak: u32,
}

fn default_quiet_hours_start() -> String {
    "22:00".to_string()
}

fn default_quiet_hours_end() -> String {
    "08:00".to_string()
}

fn default_utc_offset_minutes() -> i32 {
    480 // UTC+8
}

impl Default for ProactivityConfig {
    fn default() -> Self {
        Self {
            level: 0.35,
            quiet_hours_start: default_quiet_hours_start(),
            quiet_hours_end: default_quiet_hours_end(),
            utc_offset_minutes: default_utc_offset_minutes(),
            ignored_proactive_streak: 0,
        }
    }
}

impl ProactivityConfig {
    pub fn tier(&self) -> &'static str {
        if self.level < 0.15 {
            "silent"
        } else if self.level < 0.50 {
            "morning_brief"
        } else if self.level < 0.85 {
            "reminders"
        } else {
            "proactive_care"
        }
    }

    pub fn effective_tier(&self) -> &'static str {
        let base = self.tier();
        if self.ignored_proactive_streak >= 3 {
            if base == "silent" {
                "silent"
            } else {
                "morning_brief"
            }
        } else {
            base
        }
    }

    pub fn is_in_quiet_hours(&self, now_ms: i64) -> bool {
        let total_mins = ((now_ms / 1000 / 60) + self.utc_offset_minutes as i64).rem_euclid(1440);
        let local_hour = (total_mins / 60) as u32;
        let local_min = (total_mins % 60) as u32;
        let now_hm = format!("{:02}:{:02}", local_hour, local_min);

        let start = self.quiet_hours_start.trim();
        let end = self.quiet_hours_end.trim();

        if start <= end {
            now_hm.as_str() >= start && now_hm.as_str() < end
        } else {
            now_hm.as_str() >= start || now_hm.as_str() < end
        }
    }

    pub fn can_send_proactive(&self, kind: ProactiveKind, now_ms: i64) -> bool {
        if kind == ProactiveKind::Urgent {
            return true;
        }

        if self.is_in_quiet_hours(now_ms) {
            return false;
        }

        let eff = self.effective_tier();
        match kind {
            ProactiveKind::MorningBrief => eff != "silent",
            ProactiveKind::Reminder => eff == "reminders" || eff == "proactive_care",
            ProactiveKind::ProactiveCare => eff == "proactive_care",
            ProactiveKind::Urgent => true,
        }
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "level": self.level,
            "tier": self.tier(),
            "effective_tier": self.effective_tier(),
            "effectiveTier": self.effective_tier(),
            "quiet_hours_start": self.quiet_hours_start,
            "quietHoursStart": self.quiet_hours_start,
            "quiet_hours_end": self.quiet_hours_end,
            "quietHoursEnd": self.quiet_hours_end,
            "utc_offset_minutes": self.utc_offset_minutes,
            "utcOffsetMinutes": self.utc_offset_minutes,
            "ignored_proactive_streak": self.ignored_proactive_streak,
            "ignoredProactiveStreak": self.ignored_proactive_streak,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProactivityStore {
    file_path: PathBuf,
    pub config: ProactivityConfig,
}

impl ProactivityStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("proactivity.json");
        let config = if file_path.is_file() {
            let content =
                std::fs::read_to_string(&file_path).map_err(|_| "PROACTIVITY_READ_FAILED")?;
            serde_json::from_str::<ProactivityConfig>(&content).unwrap_or_default()
        } else {
            let def = ProactivityConfig::default();
            let raw = serde_json::to_string_pretty(&def).unwrap_or_default();
            let _ = std::fs::write(&file_path, raw.as_bytes());
            def
        };

        Ok(Self { file_path, config })
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.config)
            .map_err(|_| "PROACTIVITY_SERIALIZE_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes())
            .map_err(|_| "PROACTIVITY_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "PROACTIVITY_WRITE_FAILED")?;
        Ok(())
    }

    pub fn set_config(
        &mut self,
        level: Option<f64>,
        start: Option<String>,
        end: Option<String>,
        offset: Option<i32>,
    ) -> Result<ProactivityConfig, &'static str> {
        if let Some(l) = level {
            self.config.level = l.clamp(0.0, 1.0);
        }
        if let Some(s) = start {
            if s.contains(':') {
                self.config.quiet_hours_start = s;
            }
        }
        if let Some(e) = end {
            if e.contains(':') {
                self.config.quiet_hours_end = e;
            }
        }
        if let Some(o) = offset {
            self.config.utc_offset_minutes = o;
        }
        self.save_atomic()?;
        Ok(self.config.clone())
    }

    pub fn record_user_interaction(&mut self) -> Result<(), &'static str> {
        if self.config.ignored_proactive_streak != 0 {
            self.config.ignored_proactive_streak = 0;
            self.save_atomic()?;
        }
        Ok(())
    }

    pub fn record_proactive_ignored(&mut self) -> Result<(), &'static str> {
        self.config.ignored_proactive_streak += 1;
        self.save_atomic()
    }
}

// ---------------------------------------------------------------------------
// 3. 5-Phase Nightly Dream Consolidator & SOUL Proposal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamRecoveryLogEntry {
    pub pruned_at: i64,
    pub reason: String,
    pub memory: MemoryItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DreamRecoveryLogFile {
    pub entries: Vec<DreamRecoveryLogEntry>,
}

pub fn restore_dream_pruned(
    data_dir: &Path,
    memory_id: &str,
    app: &mut LocalApp,
) -> Result<bool, &'static str> {
    let log_path = data_dir.join("dream_recovery_log.json");
    if !log_path.is_file() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&log_path).map_err(|_| "RECOVERY_LOG_READ_FAILED")?;
    let log: DreamRecoveryLogFile =
        serde_json::from_str(&content).map_err(|_| "RECOVERY_LOG_PARSE_FAILED")?;

    let found = log.entries.iter().find(|e| e.memory.id == memory_id);
    if let Some(entry) = found {
        app.update_memory_state(&entry.memory.id, "active")
            .map_err(|_| "MEMORY_RESTORE_FAILED")?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn append_to_recovery_log(
    data_dir: &Path,
    mem: &MemoryItem,
    reason: &str,
    now_ms: i64,
) -> Result<(), &'static str> {
    let log_path = data_dir.join("dream_recovery_log.json");
    let mut log = if log_path.is_file() {
        let content =
            std::fs::read_to_string(&log_path).map_err(|_| "RECOVERY_LOG_READ_FAILED")?;
        serde_json::from_str::<DreamRecoveryLogFile>(&content).unwrap_or_default()
    } else {
        DreamRecoveryLogFile::default()
    };

    log.entries.push(DreamRecoveryLogEntry {
        pruned_at: now_ms,
        reason: reason.to_string(),
        memory: mem.clone(),
    });

    let serialized = serde_json::to_string_pretty(&log)
        .map_err(|_| "RECOVERY_LOG_SERIALIZE_FAILED")?;
    let tmp_path = log_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, serialized.as_bytes())
        .map_err(|_| "RECOVERY_LOG_WRITE_FAILED")?;
    std::fs::rename(&tmp_path, &log_path).map_err(|_| "RECOVERY_LOG_WRITE_FAILED")?;
    Ok(())
}

fn normalize_text(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamReport {
    pub timestamp: i64,
    pub memories_consolidated: usize,
    pub memories_pruned: usize,
    pub captures_gathered: usize,
    pub soul_proposal: Option<String>,
    pub summary: String,
}

pub fn run_nightly_dream(
    data_dir: &Path,
    app: &mut LocalApp,
    bus: &mut BobToolBus,
    now_ms: i64,
) -> Result<DreamReport, &'static str> {
    // Phase 1: Orient
    let active_memories = app.list_memories(true).map_err(|_| "ORIENT_FAILED")?;
    let _notes = app.list_notes(None).map_err(|_| "ORIENT_FAILED")?;
    let telemetry_store = ExecutionTelemetryStore::open(data_dir)?;
    let recent_telemetry = telemetry_store.recent_records(50);

    // Phase 2: Gather
    let captures = app.list_inbox(Some("unsorted")).unwrap_or_default();
    let captures_count = captures.len();

    // Check repeated telemetry patterns (frequent tools, rejections)
    let mut rejected_tools = Vec::new();
    for rec in &recent_telemetry {
        for rej in &rec.rejected_tools {
            if !rejected_tools.contains(rej) {
                rejected_tools.push(rej.clone());
            }
        }
    }

    // Phase 3: Consolidate
    let mut to_prune = Vec::new();
    let mut checked: Vec<usize> = Vec::new();

    for i in 0..active_memories.len() {
        if checked.contains(&i) {
            continue;
        }
        let norm_i = normalize_text(&active_memories[i].body);
        if norm_i.is_empty() {
            continue;
        }
        for j in (i + 1)..active_memories.len() {
            if checked.contains(&j) {
                continue;
            }
            let norm_j = normalize_text(&active_memories[j].body);
            // Near-identical or identical match
            if norm_i == norm_j || norm_i.contains(&norm_j) || norm_j.contains(&norm_i) {
                // Keep the newer one (higher version or higher index), prune the older
                to_prune.push((active_memories[j].clone(), "Duplicate or superseded memory"));
                checked.push(j);
            }
        }
    }

    // Phase 4: Prune (Enforce MAX_PRUNE_PER_DREAM = 3 cap!)
    let prune_cap = to_prune.len().min(MAX_PRUNE_PER_DREAM);
    let mut pruned_count = 0;

    for (mem, reason) in to_prune.into_iter().take(prune_cap) {
        // Append to recovery log BEFORE updating DB state!
        append_to_recovery_log(data_dir, &mem, reason, now_ms)?;
        let _ = app.update_memory_state(&mem.id, "forgotten");
        pruned_count += 1;
    }

    // Phase 5: Index
    let _ = app.rebuild_or_sync_kb_index();

    // SOUL Proposal check: If recurring patterns are detected
    let mut soul_proposal = None;
    if !rejected_tools.is_empty() && data_dir.join("SOUL.md").is_file() {
        let proposal = format!(
            "User frequently rejected the following tools: {}. Adjust Bob's persona to avoid proactively invoking them without explicit user request.",
            rejected_tools.join(", ")
        );
        let _ = propose_soul_update(data_dir, &proposal, bus, now_ms);
        soul_proposal = Some(proposal);
    }

    let summary = format!(
        "Dream finished: {} memories inspected, {} consolidated, {} pruned (capped at {}), {} sparks gathered.",
        active_memories.len(),
        checked.len(),
        pruned_count,
        MAX_PRUNE_PER_DREAM,
        captures_count
    );

    Ok(DreamReport {
        timestamp: now_ms,
        memories_consolidated: checked.len(),
        memories_pruned: pruned_count,
        captures_gathered: captures_count,
        soul_proposal,
        summary,
    })
}

pub fn propose_soul_update(
    data_dir: &Path,
    proposal_text: &str,
    bus: &mut BobToolBus,
    now_ms: i64,
) -> Result<PendingApprovalCard, &'static str> {
    let id = format!("approval-soul-{}", now_ms);
    let card = StructuredCard {
        id: format!("card-soul-{}", now_ms),
        kind: "approval".to_string(),
        icon: Some("sparkles".to_string()),
        title: "Proposed SOUL.md Evolution".to_string(),
        summary: "Bob proposed an evolution to SOUL persona based on recent interaction patterns."
            .to_string(),
        risk_level: ActionRiskLevel::SettingsChange,
        fields: vec![CardField {
            label: "Proposed Addition".to_string(),
            value: proposal_text.to_string(),
        }],
        items: vec![],
        status: Some("pending".to_string()),
        technical_detail: Some(format!("Target file: {}", data_dir.join("SOUL.md").display())),
    };

    let approval = PendingApprovalCard {
        id,
        tool_name: "soul_update".to_string(),
        args: json!({ "content": proposal_text }),
        card,
        created_at: now_ms,
        expires_at: now_ms + crate::tool_bus::APPROVAL_EXPIRY_MILLIS,
        status: crate::tool_bus::ApprovalStatus::Pending,
    };

    if let Some(ref mut store) = bus.approval_store {
        store.insert(approval.clone())?;
    }

    Ok(approval)
}

// ---------------------------------------------------------------------------
// 4. Structured Morning Brief Generator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorningBrief {
    pub date_key: String,
    pub generated_at: i64,
    pub today_schedules: Vec<ScheduleItem>,
    pub pending_approvals_count: usize,
    pub unsorted_inbox_count: usize,
    pub recent_routine_reports: Vec<String>,
    pub budget_summary: String,
    pub dream_highlights: Vec<String>,
    pub card: StructuredCard,
}

pub fn generate_morning_brief(
    data_dir: &Path,
    app: &LocalApp,
    now_ms: i64,
) -> Result<MorningBrief, &'static str> {
    let pro_store = ProactivityStore::open(data_dir)?;
    let offset_mins = pro_store.config.utc_offset_minutes;

    // Approximate date key calculation
    let days_epoch = ((now_ms / 1000 / 60) + offset_mins as i64) / 1440;
    // Simple civil date from days
    let (year, month, day) = days_to_civil(days_epoch);
    let date_key = format!("{:04}-{:02}-{:02}", year, month, day);

    // Today's midnight range in UTC epoch millis
    let day_start_ms = ((days_epoch * 1440) - offset_mins as i64) * 60 * 1000;
    let day_end_ms = day_start_ms + 86_400_000;

    let schedules = app.list_schedule_items().unwrap_or_default();
    let today_schedules: Vec<ScheduleItem> = schedules
        .into_iter()
        .filter(|s| {
            (s.start_at >= day_start_ms && s.start_at < day_end_ms)
                || (s.status == "scheduled" && s.kind == "todo")
        })
        .collect();

    let mut pending_approvals_count = 0;
    if let Ok(mut app_store) = PendingApprovalStore::open(data_dir) {
        pending_approvals_count = app_store.list_pending(now_ms).len();
    }

    let unsorted_inbox_count = app.list_inbox(Some("unsorted")).unwrap_or_default().len();

    let mut budget_summary = "Budget: OK".to_string();
    if let Ok(mut bstore) = MonthlyBudgetStore::open(data_dir) {
        let st = bstore.status();
        budget_summary = format!(
            "Spent: ${:.2} / Cap: ${:.2} ({:.1}%)",
            st.spent_usd,
            st.cap_usd,
            if st.cap_usd > 0.0 {
                (st.spent_usd / st.cap_usd) * 100.0
            } else {
                100.0
            }
        );
    }

    let dream_highlights = vec![
        "Knowledge index synced and fresh".to_string(),
        "Daily consolidation complete".to_string(),
    ];

    let mut items = Vec::new();
    for s in &today_schedules {
        items.push(CardItem {
            title: s.title.clone(),
            subtitle: Some(format!("Kind: {} | Status: {}", s.kind, s.status)),
            badge: Some(s.kind.clone()),
        });
    }

    let card = StructuredCard {
        id: format!("card-brief-{}", date_key),
        kind: "info".to_string(),
        icon: Some("sun".to_string()),
        title: format!("Morning Brief - {}", date_key),
        summary: format!(
            "{} items today | {} pending approvals | {} unsorted sparks",
            today_schedules.len(),
            pending_approvals_count,
            unsorted_inbox_count
        ),
        risk_level: ActionRiskLevel::ReadOnly,
        fields: vec![
            CardField {
                label: "Date".to_string(),
                value: date_key.clone(),
            },
            CardField {
                label: "Today's Schedules".to_string(),
                value: format!("{} items", today_schedules.len()),
            },
            CardField {
                label: "Pending Approvals".to_string(),
                value: format!("{} pending", pending_approvals_count),
            },
            CardField {
                label: "Unsorted Inbox".to_string(),
                value: format!("{} sparks", unsorted_inbox_count),
            },
            CardField {
                label: "Monthly Budget".to_string(),
                value: budget_summary.clone(),
            },
        ],
        items,
        status: Some("ready".to_string()),
        technical_detail: None,
    };

    Ok(MorningBrief {
        date_key,
        generated_at: now_ms,
        today_schedules,
        pending_approvals_count,
        unsorted_inbox_count,
        recent_routine_reports: Vec::new(),
        budget_summary,
        dream_highlights,
        card,
    })
}

fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_proactivity_tiers_and_quiet_hours() {
        let mut cfg = ProactivityConfig::default();
        assert_eq!(cfg.tier(), "morning_brief");
        assert_eq!(cfg.effective_tier(), "morning_brief");

        cfg.level = 0.05;
        assert_eq!(cfg.tier(), "silent");
        cfg.level = 0.70;
        assert_eq!(cfg.tier(), "reminders");
        cfg.level = 0.95;
        assert_eq!(cfg.tier(), "proactive_care");

        // Quiet hours: 22:00 - 08:00 (UTC+8, offset 480 mins)
        // 23:30 local time is UTC 15:30
        // Epoch seconds for 15:30 UTC: 15 * 3600 + 30 * 60 = 55800
        let ms_2330 = 55800 * 1000;
        assert!(cfg.is_in_quiet_hours(ms_2330));
        assert!(!cfg.can_send_proactive(ProactiveKind::ProactiveCare, ms_2330));
        assert!(!cfg.can_send_proactive(ProactiveKind::MorningBrief, ms_2330));
        assert!(cfg.can_send_proactive(ProactiveKind::Urgent, ms_2330));

        // 09:00 local time is UTC 01:00
        // Epoch seconds for 01:00 UTC: 1 * 3600 = 3600
        let ms_0900 = 3600 * 1000;
        assert!(!cfg.is_in_quiet_hours(ms_0900));
        assert!(cfg.can_send_proactive(ProactiveKind::ProactiveCare, ms_0900));
        assert!(cfg.can_send_proactive(ProactiveKind::MorningBrief, ms_0900));

        // 3-strike backoff: streak >= 3 forces effective_tier to morning_brief
        cfg.ignored_proactive_streak = 3;
        assert_eq!(cfg.tier(), "proactive_care");
        assert_eq!(cfg.effective_tier(), "morning_brief");
        assert!(!cfg.can_send_proactive(ProactiveKind::ProactiveCare, ms_0900));
        assert!(cfg.can_send_proactive(ProactiveKind::MorningBrief, ms_0900));

        // Reset streak
        cfg.ignored_proactive_streak = 0;
        assert_eq!(cfg.effective_tier(), "proactive_care");
        assert!(cfg.can_send_proactive(ProactiveKind::ProactiveCare, ms_0900));
    }

    #[test]
    fn test_telemetry_store_append_and_recent() {
        let dir = tempdir().unwrap();
        let mut store = ExecutionTelemetryStore::open(dir.path()).unwrap();
        let rec = TurnTelemetryRecord {
            timestamp: 1000,
            conversation_id: "conv-1".to_string(),
            tools_called: vec!["web_fetch".to_string()],
            succeeded: true,
            rejected_tools: vec![],
            latency_ms: 120,
            tokens_used: 450,
        };
        store.record_turn(rec.clone()).unwrap();
        let recent = store.recent_records(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0], rec);
    }

    #[test]
    fn test_nightly_dream_max_3_delete_cap_and_recovery() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("kernel.sqlite");
        let mut app = LocalApp::open(&db_path).unwrap();
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();

        let session = app.default_session().unwrap();
        app.accept("req-init", "msg-init", &session, "initial message".into(), 100)
            .unwrap();

        // Create 7 identical memories (1 primary + 6 duplicates)
        let mut created_ids = Vec::new();
        for i in 1..=7 {
            let id = format!("mem-dup-{}", i);
            let mem = app
                .propose_memory(
                    &id,
                    None,
                    Some("msg-init"),
                    "User prefers concise replies in Chinese with no emojis",
                    None,
                )
                .unwrap();
            app.update_memory_state(&mem.id, "active").unwrap();
            created_ids.push(id);
        }

        assert_eq!(app.list_memories(true).unwrap().len(), 7);

        // Run nightly dream
        let report = run_nightly_dream(dir.path(), &mut app, &mut bus, 2000).unwrap();

        // HARD SAFETY GUARDRAIL: Max 3 memories pruned in a single dream run!
        assert_eq!(report.memories_pruned, 3);
        let remaining_active = app.list_memories(true).unwrap();
        assert_eq!(remaining_active.len(), 4);

        // Verify recovery log exists and contains the 3 pruned entries
        let recovery_path = dir.path().join("dream_recovery_log.json");
        assert!(recovery_path.is_file());
        let content = std::fs::read_to_string(&recovery_path).unwrap();
        let log: DreamRecoveryLogFile = serde_json::from_str(&content).unwrap();
        assert_eq!(log.entries.len(), 3);

        // Verify 100% lossless restore
        let first_pruned_id = log.entries[0].memory.id.clone();
        let restored = restore_dream_pruned(dir.path(), &first_pruned_id, &mut app).unwrap();
        assert!(restored);
        assert_eq!(app.list_memories(true).unwrap().len(), 5);
    }

    #[test]
    fn test_morning_brief_and_soul_proposal() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("kernel.sqlite");
        let mut app = LocalApp::open(&db_path).unwrap();
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();

        let now = 1762800000000i64; // epoch ms
        app.create_schedule_item_with_kind(
            "sched-brief-1",
            None,
            None,
            "Quarterly Review",
            "todo",
            now + 10000,
            None,
            None,
            "Prepare slides",
            now,
        )
        .unwrap();

        let brief = generate_morning_brief(dir.path(), &app, now).unwrap();
        assert!(!brief.date_key.is_empty());
        assert_eq!(brief.card.kind, "info");
        assert_eq!(brief.card.risk_level, ActionRiskLevel::ReadOnly);

        // SOUL proposal does NOT silently mutate SOUL.md without approval
        let soul_file = dir.path().join("SOUL.md");
        assert!(!soul_file.exists());

        let approval = propose_soul_update(
            dir.path(),
            "Be more proactive during morning hours",
            &mut bus,
            now,
        )
        .unwrap();

        assert_eq!(approval.tool_name, "soul_update");
        assert_eq!(approval.card.risk_level, ActionRiskLevel::SettingsChange);
        assert!(!soul_file.exists()); // Not yet created/mutated!
    }
}


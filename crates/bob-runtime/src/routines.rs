//! Unified Time Object Scheduler, Walkthrough-to-Routine, Mandatory Trial Gate,
//! Unattended Draft Queue, and Pure-Rust Civil Calendar Math (`WP-J4a`).

use crate::tool_bus::{BobToolBus, StructuredCard, ToolBusOutcome, ToolExecutionMode};
use bob_kernel::app::LocalApp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MISSED_GRACE_MS: i64 = 2 * 60 * 60 * 1000; // 2 hours
pub const DRAFT_EXPIRY_MS: i64 = 7 * 24 * 60 * 60 * 1000; // 7 days
pub const DEFAULT_UTC_OFFSET_MINUTES: i32 = 480; // UTC+8

fn default_utc_offset_minutes() -> i32 {
    DEFAULT_UTC_OFFSET_MINUTES
}

// ---------------------------------------------------------------------------
// Pure-Rust Civil Calendar & Time Math
// ---------------------------------------------------------------------------

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 => 31,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 30,
    }
}

pub fn ymd_to_days(year: i32, month: u8, day: u8) -> i64 {
    let mut days = 0i64;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    for m in 1..month {
        days += days_in_month(year, m) as i64;
    }
    days += (day as i64) - 1;
    days
}

pub fn days_to_ymd(mut days: i64) -> (i32, u8, u8) {
    let mut year = 1970;
    if days >= 0 {
        loop {
            let leap = is_leap_year(year);
            let ydays = if leap { 366 } else { 365 };
            if days < ydays {
                break;
            }
            days -= ydays;
            year += 1;
        }
    } else {
        loop {
            year -= 1;
            let leap = is_leap_year(year);
            let ydays = if leap { 366 } else { 365 };
            days += ydays;
            if days >= 0 {
                break;
            }
        }
    }
    let mut month = 1;
    while month <= 12 {
        let dim = days_in_month(year, month) as i64;
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    (year, month, (days + 1) as u8)
}

/// Converts epoch ms + timezone offset into civil date & time components.
/// Weekday: 0 = Sunday .. 6 = Saturday (1970-01-01 was Thursday = 4).
pub fn epoch_ms_to_civil(
    epoch_ms: i64,
    utc_offset_minutes: i32,
) -> (i32, u8, u8, u8, u8, u8, u16, u8) {
    let local_ms = epoch_ms + (utc_offset_minutes as i64) * 60 * 1000;
    let (total_days, day_ms) = if local_ms >= 0 {
        (local_ms / 86_400_000, local_ms % 86_400_000)
    } else {
        let neg_days = (local_ms - 86_399_999) / 86_400_000;
        let rem = local_ms - neg_days * 86_400_000;
        (neg_days, rem)
    };

    let (year, month, day) = days_to_ymd(total_days);
    let total_seconds = (day_ms / 1000) as u32;
    let millisecond = (day_ms % 1000) as u16;
    let hour = (total_seconds / 3600) as u8;
    let minute = ((total_seconds % 3600) / 60) as u8;
    let second = (total_seconds % 60) as u8;
    let weekday = (((total_days % 7) + 4 + 7) % 7) as u8;

    (
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
        weekday,
    )
}

pub fn civil_to_epoch_ms(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    millisecond: u16,
    utc_offset_minutes: i32,
) -> i64 {
    let days = ymd_to_days(year, month, day);
    let local_seconds = days * 86400
        + (hour as i64) * 3600
        + (minute as i64) * 60
        + (second as i64);
    let utc_seconds = local_seconds - (utc_offset_minutes as i64) * 60;
    utc_seconds * 1000 + (millisecond as i64)
}

// ---------------------------------------------------------------------------
// Recurrence Rules & Calendar Math
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecurrenceRule {
    Once {
        #[serde(alias = "runAtMs", alias = "run_at")]
        run_at_ms: i64,
    },
    Daily {
        hour: u8,
        minute: u8,
    },
    Weekly {
        weekday: u8, // 0 = Sun .. 6 = Sat
        hour: u8,
        minute: u8,
    },
    Monthly {
        day: u8, // 1 .. 31 (clamped to month end)
        hour: u8,
        minute: u8,
    },
}

/// Computes the exact next occurrence strictly > `after_ms` honoring timezone `utc_offset_minutes`.
pub fn compute_next_run_at(
    recurrence: &RecurrenceRule,
    utc_offset_minutes: i32,
    after_ms: i64,
) -> Option<i64> {
    match recurrence {
        RecurrenceRule::Once { run_at_ms } => {
            if *run_at_ms > after_ms {
                Some(*run_at_ms)
            } else {
                None
            }
        }
        RecurrenceRule::Daily { hour, minute } => {
            let (y, m, d, _, _, _, _, _) = epoch_ms_to_civil(after_ms, utc_offset_minutes);
            let today_cand =
                civil_to_epoch_ms(y, m, d, *hour, *minute, 0, 0, utc_offset_minutes);
            if today_cand > after_ms {
                return Some(today_cand);
            }
            let tomorrow_days = ymd_to_days(y, m, d) + 1;
            let (ty, tm, td) = days_to_ymd(tomorrow_days);
            Some(civil_to_epoch_ms(
                ty,
                tm,
                td,
                *hour,
                *minute,
                0,
                0,
                utc_offset_minutes,
            ))
        }
        RecurrenceRule::Weekly {
            weekday,
            hour,
            minute,
        } => {
            let (y, m, d, _, _, _, _, cur_w) = epoch_ms_to_civil(after_ms, utc_offset_minutes);
            let target_w = *weekday % 7;
            for offset_days in 0..=7 {
                let w = (cur_w + (offset_days as u8)) % 7;
                if w == target_w {
                    let cand_days = ymd_to_days(y, m, d) + (offset_days as i64);
                    let (cy, cm, cd) = days_to_ymd(cand_days);
                    let cand_ms = civil_to_epoch_ms(
                        cy,
                        cm,
                        cd,
                        *hour,
                        *minute,
                        0,
                        0,
                        utc_offset_minutes,
                    );
                    if cand_ms > after_ms {
                        return Some(cand_ms);
                    }
                }
            }
            None
        }
        RecurrenceRule::Monthly { day, hour, minute } => {
            let (mut y, mut m, _, _, _, _, _, _) =
                epoch_ms_to_civil(after_ms, utc_offset_minutes);
            for _ in 0..24 {
                let dim = days_in_month(y, m);
                let clamped_d = (*day).clamp(1, dim);
                let cand_ms = civil_to_epoch_ms(
                    y,
                    m,
                    clamped_d,
                    *hour,
                    *minute,
                    0,
                    0,
                    utc_offset_minutes,
                );
                if cand_ms > after_ms {
                    return Some(cand_ms);
                }
                if m == 12 {
                    m = 1;
                    y += 1;
                } else {
                    m += 1;
                }
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Routine & Report Data Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineStep {
    pub step_index: usize,
    pub tool_name: String,
    pub description: String,
    pub args_template: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineRunReport {
    pub run_id: String,
    pub routine_id: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub is_trial: bool,
    pub goal_achieved: bool,
    pub summary: String,
    pub steps_executed: usize,
    pub drafts_created: Vec<String>,
    pub needs_reauth: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineRecord {
    pub id: String,
    pub title: String,
    pub prompt_instruction: String,
    #[serde(default)]
    pub routine_steps: Vec<RoutineStep>,
    pub recurrence: RecurrenceRule,
    #[serde(default = "default_utc_offset_minutes")]
    pub utc_offset_minutes: i32,
    #[serde(default)]
    pub trial_passed: bool,
    #[serde(default)]
    pub enabled: bool,
    pub next_run_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub last_status: Option<String>,
    pub last_report: Option<RoutineRunReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_id: Option<String>,
}

pub fn capture_walkthrough_to_routine(
    id: Option<&str>,
    title: &str,
    prompt_instruction: &str,
    recurrence: RecurrenceRule,
    steps: Vec<RoutineStep>,
    utc_offset_minutes: Option<i32>,
) -> RoutineRecord {
    let now = crate::now_millis();
    let offset = utc_offset_minutes.unwrap_or(DEFAULT_UTC_OFFSET_MINUTES);
    let next_run = compute_next_run_at(&recurrence, offset, now);
    RoutineRecord {
        id: id
            .map(str::to_string)
            .unwrap_or_else(|| format!("rtn-{}", now)),
        title: title.to_string(),
        prompt_instruction: prompt_instruction.to_string(),
        routine_steps: steps,
        recurrence,
        utc_offset_minutes: offset,
        trial_passed: false,
        enabled: false,
        next_run_at: next_run,
        last_run_at: None,
        last_status: None,
        last_report: None,
        schedule_id: None,
    }
}

// ---------------------------------------------------------------------------
// Unattended Draft Queue Store
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineDraftRecord {
    pub id: String,
    pub routine_id: String,
    pub routine_title: String,
    pub tool_name: String,
    pub args: Value,
    pub card: StructuredCard,
    pub status: String, // "pending" | "approved" | "rejected" | "expired"
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DraftStoreData {
    pub drafts: Vec<RoutineDraftRecord>,
}

#[derive(Debug, Clone)]
pub struct UnattendedDraftStore {
    file_path: PathBuf,
    data: DraftStoreData,
}

impl UnattendedDraftStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("routine_drafts.json");

        let data = if file_path.is_file() {
            let content =
                std::fs::read_to_string(&file_path).map_err(|_| "ROUTINE_DRAFTS_READ_FAILED")?;
            serde_json::from_str::<DraftStoreData>(&content).unwrap_or_default()
        } else {
            DraftStoreData::default()
        };

        let store = Self { file_path, data };
        let _ = store.save_atomic();
        Ok(store)
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "ROUTINE_DRAFTS_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes())
            .map_err(|_| "ROUTINE_DRAFTS_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "ROUTINE_DRAFTS_WRITE_FAILED")?;
        Ok(())
    }

    pub fn insert(&mut self, draft: RoutineDraftRecord) -> Result<(), &'static str> {
        if let Some(pos) = self.data.drafts.iter().position(|d| d.id == draft.id) {
            self.data.drafts[pos] = draft;
        } else {
            self.data.drafts.push(draft);
        }
        self.save_atomic()
    }

    pub fn get(&self, id: &str) -> Option<&RoutineDraftRecord> {
        self.data.drafts.iter().find(|d| d.id == id)
    }

    pub fn list_pending(&mut self, now_ms: i64) -> Vec<RoutineDraftRecord> {
        let mut modified = false;
        for d in &mut self.data.drafts {
            if d.status == "pending" && now_ms >= d.expires_at {
                d.status = "expired".to_string();
                modified = true;
            }
        }
        if modified {
            let _ = self.save_atomic();
        }
        self.data
            .drafts
            .iter()
            .filter(|d| d.status == "pending")
            .cloned()
            .collect()
    }

    pub fn list_all(&mut self, now_ms: i64) -> Vec<RoutineDraftRecord> {
        let mut modified = false;
        for d in &mut self.data.drafts {
            if d.status == "pending" && now_ms >= d.expires_at {
                d.status = "expired".to_string();
                modified = true;
            }
        }
        if modified {
            let _ = self.save_atomic();
        }
        self.data.drafts.clone()
    }

    /// Replays the pending draft deterministically through BobToolBus without calling any LLM.
    pub fn approve_draft_once(
        &mut self,
        draft_id: &str,
        bus: &BobToolBus,
        app: &mut LocalApp,
        now_ms: i64,
    ) -> Result<Value, &'static str> {
        let draft = self
            .data
            .drafts
            .iter_mut()
            .find(|d| d.id == draft_id)
            .ok_or("DRAFT_NOT_FOUND")?;

        if draft.status == "expired" || now_ms >= draft.expires_at {
            if draft.status != "expired" {
                draft.status = "expired".to_string();
                let _ = self.save_atomic();
            }
            return Err("DRAFT_EXPIRED");
        }
        if draft.status != "pending" {
            return Err("DRAFT_NOT_PENDING");
        }

        let outcome = bus
            .execute_bypassing_approval(&draft.tool_name, &draft.args, app)
            .map_err(|_| "DRAFT_EXECUTION_FAILED")?;

        draft.status = "approved".to_string();
        let _ = self.save_atomic();

        let output = match outcome {
            ToolBusOutcome::Executed { output, .. } => output,
            _ => json!({ "status": "approved", "draft_id": draft_id }),
        };
        Ok(output)
    }

    pub fn reject_draft(&mut self, draft_id: &str, _now_ms: i64) -> Result<(), &'static str> {
        let draft = self
            .data
            .drafts
            .iter_mut()
            .find(|d| d.id == draft_id)
            .ok_or("DRAFT_NOT_FOUND")?;

        if draft.status != "pending" {
            return Err("DRAFT_NOT_PENDING");
        }
        draft.status = "rejected".to_string();
        self.save_atomic()
    }
}

// ---------------------------------------------------------------------------
// Routine Store & Scheduler Engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RoutineStoreData {
    pub routines: Vec<RoutineRecord>,
}

#[derive(Debug, Clone)]
pub struct RoutineStore {
    file_path: PathBuf,
    data: RoutineStoreData,
}

impl RoutineStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("routines_store.json");

        let data = if file_path.is_file() {
            let content =
                std::fs::read_to_string(&file_path).map_err(|_| "ROUTINES_READ_FAILED")?;
            serde_json::from_str::<RoutineStoreData>(&content).unwrap_or_default()
        } else {
            RoutineStoreData::default()
        };

        let store = Self { file_path, data };
        let _ = store.save_atomic();
        Ok(store)
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "ROUTINES_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes()).map_err(|_| "ROUTINES_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "ROUTINES_WRITE_FAILED")?;
        Ok(())
    }

    pub fn list(&self) -> Vec<RoutineRecord> {
        self.data.routines.clone()
    }

    pub fn get(&self, id: &str) -> Option<&RoutineRecord> {
        self.data.routines.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut RoutineRecord> {
        self.data.routines.iter_mut().find(|r| r.id == id)
    }

    pub fn save_routine(&mut self, mut routine: RoutineRecord) -> Result<RoutineRecord, &'static str> {
        let now = crate::now_millis();
        if let Some(pos) = self.data.routines.iter().position(|r| r.id == routine.id) {
            let old = &self.data.routines[pos];
            // If instructions or steps changed, trial must be re-run!
            let steps_changed = old.routine_steps != routine.routine_steps
                || old.prompt_instruction != routine.prompt_instruction;
            if steps_changed {
                routine.trial_passed = false;
                routine.enabled = false;
            } else {
                routine.trial_passed = old.trial_passed;
                routine.enabled = old.enabled;
            }
            if routine.next_run_at.is_none() {
                routine.next_run_at =
                    compute_next_run_at(&routine.recurrence, routine.utc_offset_minutes, now);
            }
            self.data.routines[pos] = routine.clone();
        } else {
            routine.trial_passed = false;
            routine.enabled = false;
            if routine.next_run_at.is_none() {
                routine.next_run_at =
                    compute_next_run_at(&routine.recurrence, routine.utc_offset_minutes, now);
            }
            self.data.routines.push(routine.clone());
        }
        self.save_atomic()?;
        Ok(routine)
    }

    pub fn delete_routine(&mut self, id: &str) -> Result<RoutineRecord, &'static str> {
        let pos = self
            .data
            .routines
            .iter()
            .position(|r| r.id == id)
            .ok_or("ROUTINE_NOT_FOUND")?;
        let removed = self.data.routines.remove(pos);
        self.save_atomic()?;
        Ok(removed)
    }

    /// Hard invariant: Enabling a routine MUST fail with `ROUTINE_TRIAL_REQUIRED` unless `trial_passed == true`.
    pub fn enable_routine(
        &mut self,
        id: &str,
        enabled: bool,
        now_ms: i64,
    ) -> Result<RoutineRecord, &'static str> {
        let routine = self
            .data
            .routines
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("ROUTINE_NOT_FOUND")?;

        if enabled {
            if !routine.trial_passed {
                return Err("ROUTINE_TRIAL_REQUIRED");
            }
            routine.enabled = true;
            routine.next_run_at =
                compute_next_run_at(&routine.recurrence, routine.utc_offset_minutes, now_ms);
        } else {
            routine.enabled = false;
        }

        let res = routine.clone();
        self.save_atomic()?;
        Ok(res)
    }

    /// Runs routine in trial mode (`ToolExecutionMode::UnattendedRoutine`), captures draft previews,
    /// and sets `trial_passed = true` if goal achieved.
    pub fn run_trial(
        &mut self,
        routine_id: &str,
        bus: &mut BobToolBus,
        draft_store: &mut UnattendedDraftStore,
        app: &mut LocalApp,
        now_ms: i64,
    ) -> Result<(RoutineRecord, RoutineRunReport, Vec<RoutineDraftRecord>), &'static str> {
        let routine = self
            .data
            .routines
            .iter_mut()
            .find(|r| r.id == routine_id)
            .ok_or("ROUTINE_NOT_FOUND")?;

        let run_id = format!("trial-{}", now_ms);
        let started_at = now_ms;
        let mut steps_executed = 0;
        let mut drafts_created = Vec::new();
        let mut captured_draft_records = Vec::new();
        let mut goal_achieved = true;
        let mut needs_reauth = false;
        let mut error = None;

        for step in &routine.routine_steps {
            match bus.execute(
                &step.tool_name,
                &step.args_template,
                ToolExecutionMode::UnattendedRoutine,
                app,
            ) {
                Ok(ToolBusOutcome::Executed { .. }) => {
                    steps_executed += 1;
                }
                Ok(ToolBusOutcome::SavedAsDraft { draft }) => {
                    steps_executed += 1;
                    drafts_created.push(draft.id.clone());
                    let draft_rec = RoutineDraftRecord {
                        id: draft.id.clone(),
                        routine_id: routine.id.clone(),
                        routine_title: routine.title.clone(),
                        tool_name: draft.tool_name.clone(),
                        args: draft.args.clone(),
                        card: draft.card.clone(),
                        status: "pending".to_string(),
                        created_at: now_ms,
                        expires_at: now_ms + DRAFT_EXPIRY_MS,
                    };
                    let _ = draft_store.insert(draft_rec.clone());
                    captured_draft_records.push(draft_rec);
                }
                Ok(ToolBusOutcome::RequiresApproval { .. }) => {
                    steps_executed += 1;
                }
                Err(e) => {
                    goal_achieved = false;
                    let err_lower = e.to_lowercase();
                    if err_lower.contains("login")
                        || err_lower.contains("auth")
                        || err_lower.contains("takeover")
                    {
                        needs_reauth = true;
                    }
                    error = Some(e);
                    break;
                }
            }
        }

        let finished_at = crate::now_millis();
        let summary = if goal_achieved {
            if !drafts_created.is_empty() {
                format!(
                    "Trial run executed {steps_executed} steps cleanly with {} action draft(s) queued for preview",
                    drafts_created.len()
                )
            } else {
                format!("Trial run completed successfully ({steps_executed} steps executed)")
            }
        } else {
            format!(
                "Trial run failed at step {steps_executed}: {}",
                error.as_deref().unwrap_or("unknown error")
            )
        };

        let report = RoutineRunReport {
            run_id,
            routine_id: routine.id.clone(),
            started_at,
            finished_at,
            is_trial: true,
            goal_achieved,
            summary,
            steps_executed,
            drafts_created,
            needs_reauth,
            error,
        };

        if goal_achieved {
            routine.trial_passed = true;
            routine.last_status = if !captured_draft_records.is_empty() {
                Some("draft_queued".to_string())
            } else {
                Some("succeeded".to_string())
            };
        } else if needs_reauth {
            routine.enabled = false;
            routine.last_status = Some("paused_needs_login".to_string());
        } else {
            routine.last_status = Some("failed".to_string());
        }
        routine.last_run_at = Some(started_at);
        routine.last_report = Some(report.clone());

        let routine_copy = routine.clone();
        self.save_atomic()?;
        Ok((routine_copy, report, captured_draft_records))
    }

    /// Evaluates due routines against current timestamp `now_ms`.
    /// Enforces the 2-hour missed grace window rule:
    /// - If `now_ms - next_run_at > 2h`: marks `missed`, skips stale side effects, advances to next occurrence.
    /// - If `0 <= now_ms - next_run_at <= 2h`: executes due routine unattended.
    pub fn tick_due_routines(
        &mut self,
        now_ms: i64,
        bus: &mut BobToolBus,
        draft_store: &mut UnattendedDraftStore,
        app: &mut LocalApp,
    ) -> Vec<RoutineRunReport> {
        let mut reports = Vec::new();
        let mut dirty = false;

        for routine in &mut self.data.routines {
            if !routine.enabled {
                continue;
            }
            let Some(next_run) = routine.next_run_at else {
                continue;
            };
            if now_ms < next_run {
                continue;
            }

            dirty = true;
            let diff = now_ms - next_run;

            if diff > MISSED_GRACE_MS {
                let report = RoutineRunReport {
                    run_id: format!("run-missed-{}", now_ms),
                    routine_id: routine.id.clone(),
                    started_at: now_ms,
                    finished_at: now_ms,
                    is_trial: false,
                    goal_achieved: false,
                    summary: format!(
                        "Execution window missed by {} minutes (> 2 hours grace expired). Stale execution avoided.",
                        diff / (60 * 1000)
                    ),
                    steps_executed: 0,
                    drafts_created: Vec::new(),
                    needs_reauth: false,
                    error: Some("MISSED_EXECUTION_WINDOW".to_string()),
                };
                routine.last_status = Some("missed".to_string());
                routine.last_run_at = Some(now_ms);
                routine.last_report = Some(report.clone());
                match routine.recurrence {
                    RecurrenceRule::Once { .. } => {
                        routine.enabled = false;
                        routine.next_run_at = None;
                    }
                    _ => {
                        routine.next_run_at = compute_next_run_at(
                            &routine.recurrence,
                            routine.utc_offset_minutes,
                            now_ms,
                        );
                    }
                }
                reports.push(report);
            } else {
                let started_at = now_ms;
                let mut steps_executed = 0;
                let mut drafts_created = Vec::new();
                let mut goal_achieved = true;
                let mut needs_reauth = false;
                let mut error = None;

                for step in &routine.routine_steps {
                    match bus.execute(
                        &step.tool_name,
                        &step.args_template,
                        ToolExecutionMode::UnattendedRoutine,
                        app,
                    ) {
                        Ok(ToolBusOutcome::Executed { .. }) => {
                            steps_executed += 1;
                        }
                        Ok(ToolBusOutcome::SavedAsDraft { draft }) => {
                            steps_executed += 1;
                            drafts_created.push(draft.id.clone());
                            let draft_rec = RoutineDraftRecord {
                                id: draft.id.clone(),
                                routine_id: routine.id.clone(),
                                routine_title: routine.title.clone(),
                                tool_name: draft.tool_name.clone(),
                                args: draft.args.clone(),
                                card: draft.card.clone(),
                                status: "pending".to_string(),
                                created_at: now_ms,
                                expires_at: now_ms + DRAFT_EXPIRY_MS,
                            };
                            let _ = draft_store.insert(draft_rec);
                        }
                        Ok(ToolBusOutcome::RequiresApproval { .. }) => {
                            steps_executed += 1;
                        }
                        Err(e) => {
                            goal_achieved = false;
                            let err_lower = e.to_lowercase();
                            if err_lower.contains("login")
                                || err_lower.contains("auth")
                                || err_lower.contains("takeover")
                            {
                                needs_reauth = true;
                            }
                            error = Some(e);
                            break;
                        }
                    }
                }

                let finished_at = crate::now_millis();
                let summary = if goal_achieved {
                    if !drafts_created.is_empty() {
                        format!(
                            "Routine executed with {} draft(s) queued for confirmation",
                            drafts_created.len()
                        )
                    } else {
                        format!("Routine executed successfully ({steps_executed} steps)")
                    }
                } else {
                    format!(
                        "Routine failed at step {steps_executed}: {}",
                        error.as_deref().unwrap_or("unknown error")
                    )
                };

                let report = RoutineRunReport {
                    run_id: format!("run-{}", now_ms),
                    routine_id: routine.id.clone(),
                    started_at,
                    finished_at,
                    is_trial: false,
                    goal_achieved,
                    summary,
                    steps_executed,
                    drafts_created: drafts_created.clone(),
                    needs_reauth,
                    error,
                };

                routine.last_run_at = Some(started_at);
                routine.last_report = Some(report.clone());

                if needs_reauth {
                    routine.enabled = false;
                    routine.last_status = Some("paused_needs_login".to_string());
                } else if !goal_achieved {
                    routine.last_status = Some("failed".to_string());
                } else if !drafts_created.is_empty() {
                    routine.last_status = Some("draft_queued".to_string());
                } else {
                    routine.last_status = Some("succeeded".to_string());
                }

                match routine.recurrence {
                    RecurrenceRule::Once { .. } => {
                        routine.enabled = false;
                        routine.next_run_at = None;
                    }
                    _ => {
                        routine.next_run_at = compute_next_run_at(
                            &routine.recurrence,
                            routine.utc_offset_minutes,
                            now_ms,
                        );
                    }
                }
                reports.push(report);
            }
        }

        if dirty {
            let _ = self.save_atomic();
        }
        reports
    }
}

// ---------------------------------------------------------------------------
// ScheduleItem Timeline Mirroring
// ---------------------------------------------------------------------------

pub fn mirror_routine_to_schedule(
    app: &mut LocalApp,
    routine: &RoutineRecord,
    now_ms: i64,
) -> Result<Option<String>, &'static str> {
    let sched_id = format!("sched-rtn-{}", routine.id);
    let title = format!("[Routine] {}", routine.title);
    let start_at = routine.next_run_at.unwrap_or(now_ms);
    let notes = format!("routine_id:{}", routine.id);

    let items = app
        .list_schedule_items()
        .map_err(|_| "SCHEDULE_LIST_FAILED")?;
    if let Some(existing) = items.into_iter().find(|s| s.id == sched_id) {
        if existing.title != title || existing.start_at != start_at || existing.notes != notes {
            let _ = app.edit_schedule_item(
                &sched_id,
                existing.version,
                &title,
                start_at,
                &notes,
                now_ms,
            );
        }
        let target_status = if routine.enabled {
            "scheduled"
        } else {
            "completed"
        };
        if existing.status != target_status {
            let _ = app.update_schedule_status(&sched_id, target_status, now_ms);
        }
    } else {
        let _ = app.create_schedule_item_with_kind(
            &sched_id,
            None,
            None,
            &title,
            "todo",
            start_at,
            None,
            Some(start_at),
            &notes,
            now_ms,
        );
        if !routine.enabled {
            let _ = app.update_schedule_status(&sched_id, "completed", now_ms);
        }
    }

    Ok(Some(sched_id))
}

pub fn unmirror_routine_from_schedule(
    app: &mut LocalApp,
    routine: &RoutineRecord,
    now_ms: i64,
) -> Result<(), &'static str> {
    let sched_id = format!("sched-rtn-{}", routine.id);
    let _ = app.delete_schedule_item(&sched_id, now_ms);
    Ok(())
}

// ---------------------------------------------------------------------------
// Headless Server Scheduler Hook
// ---------------------------------------------------------------------------

pub fn tick_due_server(
    data_dir: &Path,
    app: &mut LocalApp,
    now_ms: i64,
) -> Result<Vec<RoutineRunReport>, &'static str> {
    let mut routine_store = RoutineStore::open(data_dir)?;
    let mut draft_store = UnattendedDraftStore::open(data_dir)?;
    let mut bus =
        BobToolBus::with_approval_store(data_dir).map_err(|_| "BUS_INIT_FAILED")?;
    let reports = routine_store.tick_due_routines(now_ms, &mut bus, &mut draft_store, app);
    for r in routine_store.list() {
        let _ = mirror_routine_to_schedule(app, &r, now_ms);
    }
    Ok(reports)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_bus::ActionRiskLevel;

    fn create_test_app(dir: &Path) -> LocalApp {
        let db_path = dir.join("test_kernel.sqlite");
        LocalApp::open(&db_path).expect("open test local app")
    }

    #[test]
    fn test_civil_calendar_math_and_leap_year() {
        // 2024 is a leap year (Feb has 29 days)
        assert!(is_leap_year(2024));
        assert_eq!(days_in_month(2024, 2), 29);

        // 2025 and 2026 are not leap years
        assert!(!is_leap_year(2025));
        assert!(!is_leap_year(2026));
        assert_eq!(days_in_month(2026, 2), 28);

        // Test epoch conversions round-trip
        let epoch = 1700000000000i64; // arbitrary timestamp
        let offset = 480; // UTC+8
        let (y, m, d, h, min, s, ms, _w) = epoch_ms_to_civil(epoch, offset);
        let back = civil_to_epoch_ms(y, m, d, h, min, s, ms, offset);
        assert_eq!(epoch, back);
    }

    #[test]
    fn test_recurrence_once_and_daily() {
        let offset = 480;
        let base_epoch = civil_to_epoch_ms(2026, 11, 10, 10, 0, 0, 0, offset);

        // Once: future returns timestamp, past returns None
        let once_fut = RecurrenceRule::Once {
            run_at_ms: base_epoch + 1000,
        };
        assert_eq!(
            compute_next_run_at(&once_fut, offset, base_epoch),
            Some(base_epoch + 1000)
        );
        let once_past = RecurrenceRule::Once {
            run_at_ms: base_epoch - 1000,
        };
        assert_eq!(compute_next_run_at(&once_past, offset, base_epoch), None);

        // Daily: at 18:00 (today, future)
        let daily_today = RecurrenceRule::Daily { hour: 18, minute: 0 };
        let expected_today = civil_to_epoch_ms(2026, 11, 10, 18, 0, 0, 0, offset);
        assert_eq!(
            compute_next_run_at(&daily_today, offset, base_epoch),
            Some(expected_today)
        );

        // Daily: at 08:00 (already passed today -> must be tomorrow)
        let daily_tom = RecurrenceRule::Daily { hour: 8, minute: 0 };
        let expected_tom = civil_to_epoch_ms(2026, 11, 11, 8, 0, 0, 0, offset);
        assert_eq!(
            compute_next_run_at(&daily_tom, offset, base_epoch),
            Some(expected_tom)
        );
    }

    #[test]
    fn test_recurrence_weekly() {
        let offset = 480;
        // 2026-11-10 is a Tuesday (weekday 2)
        let base_epoch = civil_to_epoch_ms(2026, 11, 10, 12, 0, 0, 0, offset);
        let (_, _, _, _, _, _, _, w) = epoch_ms_to_civil(base_epoch, offset);
        assert_eq!(w, 2);

        // Same day Tuesday at 18:00 (later today)
        let weekly_tue_later = RecurrenceRule::Weekly {
            weekday: 2,
            hour: 18,
            minute: 0,
        };
        let exp_tue_later = civil_to_epoch_ms(2026, 11, 10, 18, 0, 0, 0, offset);
        assert_eq!(
            compute_next_run_at(&weekly_tue_later, offset, base_epoch),
            Some(exp_tue_later)
        );

        // Tuesday at 09:00 (already passed today -> next Tuesday 2026-11-17)
        let weekly_tue_past = RecurrenceRule::Weekly {
            weekday: 2,
            hour: 9,
            minute: 0,
        };
        let exp_tue_next = civil_to_epoch_ms(2026, 11, 17, 9, 0, 0, 0, offset);
        assert_eq!(
            compute_next_run_at(&weekly_tue_past, offset, base_epoch),
            Some(exp_tue_next)
        );

        // Friday (weekday 5) -> this Friday 2026-11-13
        let weekly_fri = RecurrenceRule::Weekly {
            weekday: 5,
            hour: 10,
            minute: 0,
        };
        let exp_fri = civil_to_epoch_ms(2026, 11, 13, 10, 0, 0, 0, offset);
        assert_eq!(
            compute_next_run_at(&weekly_fri, offset, base_epoch),
            Some(exp_fri)
        );
    }

    #[test]
    fn test_recurrence_monthly_month_end_clamping() {
        let offset = 480;
        // 2026-01-31 at 12:00:00
        let base_jan31 = civil_to_epoch_ms(2026, 1, 31, 12, 0, 0, 0, offset);

        // Monthly on the 31st at 10:00
        // Jan 31 10:00 already passed -> next occurrence is February!
        // 2026 Feb has 28 days -> clamped to 28!
        let monthly_31 = RecurrenceRule::Monthly {
            day: 31,
            hour: 10,
            minute: 0,
        };
        let next_feb = compute_next_run_at(&monthly_31, offset, base_jan31).unwrap();
        let (y, m, d, h, min, _, _, _) = epoch_ms_to_civil(next_feb, offset);
        assert_eq!((y, m, d, h, min), (2026, 2, 28, 10, 0));

        // And from Feb 28 12:00:00 -> March 31 has 31 days!
        let base_feb28 = civil_to_epoch_ms(2026, 2, 28, 12, 0, 0, 0, offset);
        let next_mar = compute_next_run_at(&monthly_31, offset, base_feb28).unwrap();
        let (y, m, d, h, min, _, _, _) = epoch_ms_to_civil(next_mar, offset);
        assert_eq!((y, m, d, h, min), (2026, 3, 31, 10, 0));
    }

    #[test]
    fn test_mandatory_trial_gate_invariant() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RoutineStore::open(dir.path()).unwrap();

        let routine = capture_walkthrough_to_routine(
            Some("rtn-test-1"),
            "Morning News Digest",
            "Fetch morning tech news and draft summary",
            RecurrenceRule::Daily { hour: 8, minute: 0 },
            vec![RoutineStep {
                step_index: 0,
                tool_name: "inbox_capture".to_string(),
                description: "Capture morning digest spark".to_string(),
                args_template: json!({ "content": "Morning digest" }),
            }],
            Some(480),
        );
        store.save_routine(routine).unwrap();

        // Hard Invariant: enabling before trial MUST fail with ROUTINE_TRIAL_REQUIRED
        let err = store.enable_routine("rtn-test-1", true, crate::now_millis()).unwrap_err();
        assert_eq!(err, "ROUTINE_TRIAL_REQUIRED");
    }

    #[test]
    fn test_trial_run_and_enable_success() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut store = RoutineStore::open(dir.path()).unwrap();
        let mut draft_store = UnattendedDraftStore::open(dir.path()).unwrap();
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();
        let now = crate::now_millis();

        let routine = capture_walkthrough_to_routine(
            Some("rtn-test-trial"),
            "Daily Standup Todo",
            "Add a reminder to attend daily standup",
            RecurrenceRule::Daily { hour: 9, minute: 30 },
            vec![RoutineStep {
                step_index: 0,
                tool_name: "todo_add".to_string(),
                description: "Add standup todo".to_string(),
                args_template: json!({ "title": "Attend daily team standup" }),
            }],
            Some(480),
        );
        store.save_routine(routine).unwrap();

        // Run trial
        let (updated, report, drafts) = store
            .run_trial("rtn-test-trial", &mut bus, &mut draft_store, &mut app, now)
            .unwrap();
        assert!(updated.trial_passed);
        assert!(report.goal_achieved);
        assert_eq!(report.steps_executed, 1);
        assert!(drafts.is_empty());

        // Enabling now succeeds!
        let enabled = store
            .enable_routine("rtn-test-trial", true, now)
            .expect("enable routine succeeds after trial");
        assert!(enabled.enabled);
        assert!(enabled.next_run_at.is_some());
    }

    #[test]
    fn test_unattended_routine_draft_queue_and_deterministic_replay() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut store = RoutineStore::open(dir.path()).unwrap();
        let mut draft_store = UnattendedDraftStore::open(dir.path()).unwrap();
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();
        let now = crate::now_millis();

        // 1. Create a schedule item to be deleted later
        let add_res = bus
            .execute(
                "todo_add",
                &json!({ "title": "Stale draft task to delete" }),
                ToolExecutionMode::Interactive,
                &mut app,
            )
            .unwrap();
        let item_id = match add_res {
            ToolBusOutcome::Executed { output, .. } => output["id"].as_str().unwrap().to_string(),
            _ => panic!("Expected Executed"),
        };

        // 2. Author a routine that calls schedule_delete (SideEffect)
        let routine = capture_walkthrough_to_routine(
            Some("rtn-cleaner"),
            "Nightly Stale Task Cleaner",
            "Clean stale task",
            RecurrenceRule::Daily { hour: 23, minute: 0 },
            vec![RoutineStep {
                step_index: 0,
                tool_name: "schedule_delete".to_string(),
                description: "Delete stale task".to_string(),
                args_template: json!({ "id": item_id }),
            }],
            Some(480),
        );
        store.save_routine(routine).unwrap();

        // 3. Run trial: SideEffect must become a safe SavedAsDraft
        let (_routine, report, drafts) = store
            .run_trial("rtn-cleaner", &mut bus, &mut draft_store, &mut app, now)
            .unwrap();
        assert_eq!(report.drafts_created.len(), 1);
        assert_eq!(drafts.len(), 1);
        let draft_id = &drafts[0].id;
        assert_eq!(drafts[0].card.kind, "routine_draft");
        assert_eq!(drafts[0].status, "pending");

        // Schedule item still exists before approval!
        let remaining = app.list_schedule_items().unwrap();
        assert!(remaining.iter().any(|s| s.id == item_id));

        // 4. Deterministic Draft Replay WITHOUT LLM
        let output = draft_store
            .approve_draft_once(draft_id, &bus, &mut app, now)
            .expect("deterministic replay succeeds");
        assert_eq!(output["id"], item_id);

        // Schedule item is now deleted from LocalApp!
        let after_approval = app.list_schedule_items().unwrap();
        assert!(!after_approval.iter().any(|s| s.id == item_id));

        // Draft marked approved
        let approved_draft = draft_store.get(draft_id).unwrap();
        assert_eq!(approved_draft.status, "approved");
    }

    #[test]
    fn test_draft_expiry_after_7_days() {
        let dir = tempfile::tempdir().unwrap();
        let mut draft_store = UnattendedDraftStore::open(dir.path()).unwrap();
        let mut app = create_test_app(dir.path());
        let bus = BobToolBus::with_approval_store(dir.path()).unwrap();
        let now = 1_000_000_000_000i64;

        let draft = RoutineDraftRecord {
            id: "draft-exp-1".to_string(),
            routine_id: "rtn-1".to_string(),
            routine_title: "Test".to_string(),
            tool_name: "inbox_capture".to_string(),
            args: json!({ "content": "Spark" }),
            card: StructuredCard {
                id: "card-1".to_string(),
                kind: "routine_draft".to_string(),
                icon: None,
                title: "Draft".to_string(),
                summary: "Draft".to_string(),
                risk_level: ActionRiskLevel::SideEffect,
                fields: vec![],
                items: vec![],
                status: Some("pending".to_string()),
                technical_detail: None,
            },
            status: "pending".to_string(),
            created_at: now,
            expires_at: now + DRAFT_EXPIRY_MS,
        };
        draft_store.insert(draft).unwrap();

        // 6 days later: still pending
        let pending = draft_store.list_pending(now + 6 * 24 * 60 * 60 * 1000);
        assert_eq!(pending.len(), 1);

        // 8 days later: expired!
        let pending_after = draft_store.list_pending(now + 8 * 24 * 60 * 60 * 1000);
        assert_eq!(pending_after.len(), 0);

        // Approving expired draft fails
        let err = draft_store
            .approve_draft_once("draft-exp-1", &bus, &mut app, now + 8 * 24 * 60 * 60 * 1000)
            .unwrap_err();
        assert_eq!(err, "DRAFT_EXPIRED");
    }

    #[test]
    fn test_missed_window_grace_rule() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut store = RoutineStore::open(dir.path()).unwrap();
        let mut draft_store = UnattendedDraftStore::open(dir.path()).unwrap();
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();
        let now = 1_700_000_000_000i64;

        // Schedule item due 3 hours ago (> 2h grace)
        let scheduled_time = now - (3 * 60 * 60 * 1000);
        let mut routine = capture_walkthrough_to_routine(
            Some("rtn-missed"),
            "Morning Report",
            "Generate morning report",
            RecurrenceRule::Daily { hour: 8, minute: 0 },
            vec![RoutineStep {
                step_index: 0,
                tool_name: "inbox_capture".to_string(),
                description: "Capture".to_string(),
                args_template: json!({ "content": "Morning spark" }),
            }],
            Some(480),
        );
        routine.trial_passed = true;
        routine.enabled = true;
        routine.next_run_at = Some(scheduled_time);
        store.data.routines.push(routine);
        store.save_atomic().unwrap();

        // Tick scheduler: must mark missed, avoid stale side-effects, and advance next_run_at!
        let reports = store.tick_due_routines(now, &mut bus, &mut draft_store, &mut app);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].error.as_deref(), Some("MISSED_EXECUTION_WINDOW"));

        let updated = store.get("rtn-missed").unwrap();
        assert_eq!(updated.last_status.as_deref(), Some("missed"));
        assert!(updated.next_run_at.unwrap() > now);
    }
}

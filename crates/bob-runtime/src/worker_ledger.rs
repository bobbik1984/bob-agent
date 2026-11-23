//! Durable Step Ledger & Follow-up Event Batching for Async Background Workers (`WP-E1.2`).
//!
//! Persisted at `data_dir.join("jarvis_worker_tasks.json")` under a process-wide mutex
//! and atomic temp-file rename so concurrent Resident Clerk turns, RPC readers, and
//! background Worker threads never clobber state.

use crate::native_harness::TurnUsageSummary;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const WORKER_TASKS_FILE: &str = "jarvis_worker_tasks.json";

fn ledger_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTaskStatus {
    Queued,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl WorkerTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerLedgerStep {
    pub seq: u64,
    pub kind: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchedFollowupMessage {
    pub message_id: String,
    pub content: String,
    pub queued_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerTaskRecord {
    pub task_id: String,
    pub conversation_id: String,
    pub request_id: String,
    pub title: String,
    pub objective: String,
    pub context_summary: String,
    pub status: WorkerTaskStatus,
    pub clerk_model: String,
    pub maker_model: String,
    #[serde(default)]
    pub steps: Vec<WorkerLedgerStep>,
    #[serde(default)]
    pub pending_followups: Vec<BatchedFollowupMessage>,
    #[serde(default)]
    pub cancel_requested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_text: Option<String>,
    #[serde(default)]
    pub cards: Vec<Value>,
    #[serde(default)]
    pub approvals: Vec<Value>,
    #[serde(default)]
    pub usage: TurnUsageSummary,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WorkerTaskStoreData {
    #[serde(default)]
    pub next_seq: u64,
    #[serde(default)]
    pub tasks: Vec<WorkerTaskRecord>,
}

impl WorkerTaskStoreData {
    fn alloc_seq(&mut self) -> u64 {
        if self.next_seq == 0 {
            let max_existing = self
                .tasks
                .iter()
                .flat_map(|t| t.steps.iter().map(|s| s.seq))
                .max()
                .unwrap_or(0);
            self.next_seq = max_existing;
        }
        self.next_seq += 1;
        self.next_seq
    }
}

pub struct WorkerTaskStore;

impl WorkerTaskStore {
    fn store_path(data_dir: &Path) -> PathBuf {
        data_dir.join(WORKER_TASKS_FILE)
    }

    fn load_unlocked(data_dir: &Path) -> WorkerTaskStoreData {
        let path = Self::store_path(data_dir);
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<WorkerTaskStoreData>(&raw) {
                return parsed;
            }
        }
        WorkerTaskStoreData::default()
    }

    fn save_unlocked(data_dir: &Path, data: &WorkerTaskStoreData) -> Result<(), &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let path = Self::store_path(data_dir);
        let raw =
            serde_json::to_string_pretty(data).map_err(|_| "WORKER_LEDGER_SERIALIZE_FAILED")?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, raw.as_bytes()).map_err(|_| "WORKER_LEDGER_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &path).map_err(|_| "WORKER_LEDGER_RENAME_FAILED")?;
        Ok(())
    }

    /// Creates a new `WorkerTaskRecord` in `Queued` status and appends an initial `"dispatched"` step.
    #[allow(clippy::too_many_arguments)]
    pub fn create_task(
        data_dir: &Path,
        conversation_id: &str,
        request_id: &str,
        title: &str,
        objective: &str,
        context_summary: &str,
        clerk_model: &str,
        maker_model: &str,
    ) -> WorkerTaskRecord {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let now = crate::now_millis();
        let seq = data.alloc_seq();
        let task_id = format!("wtask-{now}-{seq}");

        let initial_step = WorkerLedgerStep {
            seq,
            kind: "dispatched".to_owned(),
            summary: format!("Task dispatched: {}", title.trim()),
            detail: Some(json!({
                "title": title.trim(),
                "objective": objective.trim(),
                "maker_model": maker_model.trim(),
            })),
            timestamp: now,
        };

        let record = WorkerTaskRecord {
            task_id,
            conversation_id: conversation_id.trim().to_owned(),
            request_id: request_id.trim().to_owned(),
            title: title.trim().to_owned(),
            objective: objective.trim().to_owned(),
            context_summary: context_summary.trim().to_owned(),
            status: WorkerTaskStatus::Queued,
            clerk_model: clerk_model.trim().to_owned(),
            maker_model: maker_model.trim().to_owned(),
            steps: vec![initial_step],
            pending_followups: Vec::new(),
            cancel_requested: false,
            result_text: None,
            cards: Vec::new(),
            approvals: Vec::new(),
            usage: TurnUsageSummary::default(),
            created_at: now,
            updated_at: now,
        };

        data.tasks.push(record.clone());
        let _ = Self::save_unlocked(data_dir, &data);
        record
    }

    /// Returns a specific task by `task_id`.
    pub fn get_task(data_dir: &Path, task_id: &str) -> Option<WorkerTaskRecord> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let data = Self::load_unlocked(data_dir);
        data.tasks.into_iter().find(|t| t.task_id == task_id)
    }

    /// Returns the latest task in `Queued` or `Running` status for `conversation_id`.
    pub fn active_task_for_conversation(
        data_dir: &Path,
        conversation_id: &str,
    ) -> Option<WorkerTaskRecord> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let data = Self::load_unlocked(data_dir);
        let cid = conversation_id.trim();
        data.tasks
            .into_iter()
            .rev()
            .find(|t| {
                t.conversation_id == cid
                    && !t.cancel_requested
                    && matches!(
                        t.status,
                        WorkerTaskStatus::Queued | WorkerTaskStatus::Running
                    )
            })
    }

    /// Thread direct subscription: attaches a follow-up message directly to an active worker task
    /// without re-running the Clerk router.
    pub fn enqueue_followup(
        data_dir: &Path,
        task_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<(WorkerTaskRecord, usize), &'static str> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err("FOLLOWUP_CONTENT_EMPTY");
        }
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let task = data
            .tasks
            .iter_mut()
            .find(|t| t.task_id == task_id)
            .ok_or("WORKER_TASK_NOT_FOUND")?;

        if task.status.is_terminal() || task.cancel_requested {
            return Err("WORKER_TASK_NOT_ACTIVE");
        }

        let now = crate::now_millis();
        let mid = if message_id.trim().is_empty() {
            format!("fmsg-{now}-{}", task.pending_followups.len() + 1)
        } else {
            message_id.trim().to_owned()
        };

        if !task
            .pending_followups
            .iter()
            .any(|m| m.message_id == mid)
        {
            task.pending_followups.push(BatchedFollowupMessage {
                message_id: mid,
                content: trimmed.to_owned(),
                queued_at: now,
            });
            task.updated_at = now;
        }

        let count = task.pending_followups.len();
        let snapshot = task.clone();
        Self::save_unlocked(data_dir, &data)?;
        Ok((snapshot, count))
    }

    /// Event batching: atomically drains all queued follow-ups before the Worker's next ReAct
    /// iteration and records a `"followup_batched"` step in the ledger.
    pub fn drain_followups(data_dir: &Path, task_id: &str) -> Vec<BatchedFollowupMessage> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let Some(idx) = data.tasks.iter().position(|t| t.task_id == task_id) else {
            return Vec::new();
        };
        if data.tasks[idx].pending_followups.is_empty() {
            return Vec::new();
        }
        let seq = data.alloc_seq();
        let now = crate::now_millis();
        let drained = std::mem::take(&mut data.tasks[idx].pending_followups);
        let msg_ids: Vec<String> = drained.iter().map(|m| m.message_id.clone()).collect();
        data.tasks[idx].steps.push(WorkerLedgerStep {
            seq,
            kind: "followup_batched".to_owned(),
            summary: format!("Batched {} follow-up instruction(s)", drained.len()),
            detail: Some(json!({
                "count": drained.len(),
                "queued_count": drained.len(),
                "message_ids": msg_ids,
                "messages": &drained,
            })),
            timestamp: now,
        });
        data.tasks[idx].updated_at = now;
        let _ = Self::save_unlocked(data_dir, &data);
        drained
    }

    /// Appends a step to `task_id` and persists atomically.
    pub fn append_step(
        data_dir: &Path,
        task_id: &str,
        kind: &str,
        summary: &str,
        detail: Option<Value>,
    ) -> Option<WorkerLedgerStep> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let idx = data.tasks.iter().position(|t| t.task_id == task_id)?;
        let seq = data.alloc_seq();
        let now = crate::now_millis();
        let step = WorkerLedgerStep {
            seq,
            kind: kind.to_owned(),
            summary: summary.to_owned(),
            detail,
            timestamp: now,
        };
        data.tasks[idx].steps.push(step.clone());
        data.tasks[idx].updated_at = now;
        let _ = Self::save_unlocked(data_dir, &data);
        Some(step)
    }

    /// Records a structured card emitted by the Worker and appends a `"card"` ledger step.
    pub fn record_card(data_dir: &Path, task_id: &str, card: Value) -> Option<WorkerLedgerStep> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let idx = data.tasks.iter().position(|t| t.task_id == task_id)?;
        let seq = data.alloc_seq();
        let now = crate::now_millis();
        let title = card
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Card")
            .to_owned();
        data.tasks[idx].cards.push(card.clone());
        let step = WorkerLedgerStep {
            seq,
            kind: "card".to_owned(),
            summary: format!("Card: {title}"),
            detail: Some(card),
            timestamp: now,
        };
        data.tasks[idx].steps.push(step.clone());
        data.tasks[idx].updated_at = now;
        let _ = Self::save_unlocked(data_dir, &data);
        Some(step)
    }

    /// Records a pending approval emitted by the Worker, transitions status to `WaitingApproval`,
    /// and appends an `"approval_required"` step.
    pub fn record_approval(
        data_dir: &Path,
        task_id: &str,
        approval: Value,
    ) -> Option<WorkerLedgerStep> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let idx = data.tasks.iter().position(|t| t.task_id == task_id)?;
        let seq = data.alloc_seq();
        let now = crate::now_millis();
        let tool_name = approval
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_owned();
        data.tasks[idx].approvals.push(approval.clone());
        data.tasks[idx].status = WorkerTaskStatus::WaitingApproval;
        let step = WorkerLedgerStep {
            seq,
            kind: "approval_required".to_owned(),
            summary: format!("Approval required for {tool_name}"),
            detail: Some(approval),
            timestamp: now,
        };
        data.tasks[idx].steps.push(step.clone());
        data.tasks[idx].updated_at = now;
        let _ = Self::save_unlocked(data_dir, &data);
        Some(step)
    }

    pub fn mark_status(
        data_dir: &Path,
        task_id: &str,
        status: WorkerTaskStatus,
    ) -> Option<WorkerTaskRecord> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let task = data.tasks.iter_mut().find(|t| t.task_id == task_id)?;
        task.status = status;
        task.updated_at = crate::now_millis();
        let out = task.clone();
        let _ = Self::save_unlocked(data_dir, &data);
        Some(out)
    }

    pub fn complete_task(
        data_dir: &Path,
        task_id: &str,
        result_text: &str,
        usage: TurnUsageSummary,
    ) -> Option<WorkerTaskRecord> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let idx = data.tasks.iter().position(|t| t.task_id == task_id)?;
        let seq = data.alloc_seq();
        let now = crate::now_millis();
        let task = &mut data.tasks[idx];
        task.status = WorkerTaskStatus::Completed;
        task.result_text = Some(result_text.to_owned());
        task.usage = usage.clone();
        task.updated_at = now;
        let preview: String = result_text.chars().take(160).collect();
        task.steps.push(WorkerLedgerStep {
            seq,
            kind: "completed".to_owned(),
            summary: if preview.trim().is_empty() {
                "Task completed".to_owned()
            } else {
                preview
            },
            detail: Some(json!({ "usage": usage })),
            timestamp: now,
        });
        let out = task.clone();
        let _ = Self::save_unlocked(data_dir, &data);
        Some(out)
    }

    pub fn fail_task(
        data_dir: &Path,
        task_id: &str,
        error_summary: &str,
        usage: Option<TurnUsageSummary>,
    ) -> Option<WorkerTaskRecord> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let idx = data.tasks.iter().position(|t| t.task_id == task_id)?;
        let now = crate::now_millis();
        if data.tasks[idx].status == WorkerTaskStatus::Cancelled {
            if let Some(u) = usage {
                data.tasks[idx].usage = u;
                data.tasks[idx].updated_at = now;
                let _ = Self::save_unlocked(data_dir, &data);
            }
            return Some(data.tasks[idx].clone());
        }
        let seq = data.alloc_seq();
        let is_cancel = data.tasks[idx].cancel_requested
            || error_summary.contains("WORKER_TASK_CANCELLED");
        let task = &mut data.tasks[idx];
        if let Some(u) = usage {
            task.usage = u;
        }
        task.status = if is_cancel {
            WorkerTaskStatus::Cancelled
        } else {
            WorkerTaskStatus::Failed
        };
        task.updated_at = now;
        task.steps.push(WorkerLedgerStep {
            seq,
            kind: if is_cancel {
                "cancelled".to_owned()
            } else {
                "failed".to_owned()
            },
            summary: error_summary.to_owned(),
            detail: Some(json!({ "error": error_summary })),
            timestamp: now,
        });
        let out = task.clone();
        let _ = Self::save_unlocked(data_dir, &data);
        Some(out)
    }

    pub fn request_cancel(
        data_dir: &Path,
        task_id: &str,
    ) -> Result<WorkerTaskRecord, &'static str> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut data = Self::load_unlocked(data_dir);
        let idx = data
            .tasks
            .iter()
            .position(|t| t.task_id == task_id)
            .ok_or("WORKER_TASK_NOT_FOUND")?;

        let now = crate::now_millis();
        data.tasks[idx].cancel_requested = true;
        if !data.tasks[idx].status.is_terminal() {
            let seq = data.alloc_seq();
            data.tasks[idx].status = WorkerTaskStatus::Cancelled;
            data.tasks[idx].steps.push(WorkerLedgerStep {
                seq,
                kind: "cancelled".to_owned(),
                summary: "Task cancelled by user".to_owned(),
                detail: None,
                timestamp: now,
            });
        }
        data.tasks[idx].updated_at = now;
        let out = data.tasks[idx].clone();
        Self::save_unlocked(data_dir, &data)?;
        Ok(out)
    }

    pub fn is_cancelled(data_dir: &Path, task_id: &str) -> bool {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let data = Self::load_unlocked(data_dir);
        data.tasks
            .iter()
            .find(|t| t.task_id == task_id)
            .map(|t| t.cancel_requested || t.status == WorkerTaskStatus::Cancelled)
            .unwrap_or(false)
    }

    /// Lists worker tasks (optionally scoped to `conversation_id`), returning steps with
    /// `seq > after_seq` for seamless reconnect/poll recovery.
    pub fn list_tasks(
        data_dir: &Path,
        conversation_id: Option<&str>,
        after_seq: u64,
    ) -> Vec<Value> {
        let _guard = ledger_lock().lock().unwrap_or_else(|e| e.into_inner());
        let data = Self::load_unlocked(data_dir);
        let cid_filter = conversation_id.map(str::trim).filter(|s| !s.is_empty());

        let mut out = Vec::new();
        for task in data.tasks {
            if let Some(cid) = cid_filter {
                if task.conversation_id != cid {
                    continue;
                }
            }
            let latest_seq = task.steps.last().map(|s| s.seq).unwrap_or(0);
            let steps: Vec<WorkerLedgerStep> = if after_seq > 0 {
                task.steps
                    .into_iter()
                    .filter(|s| s.seq > after_seq)
                    .collect()
            } else {
                task.steps
            };
            if after_seq > 0 && steps.is_empty() {
                continue;
            }
            out.push(json!({
                "task_id": task.task_id,
                "taskId": task.task_id,
                "conversation_id": task.conversation_id,
                "conversationId": task.conversation_id,
                "request_id": task.request_id,
                "requestId": task.request_id,
                "title": task.title,
                "objective": task.objective,
                "context_summary": task.context_summary,
                "contextSummary": task.context_summary,
                "status": task.status,
                "clerk_model": task.clerk_model,
                "clerkModel": task.clerk_model,
                "maker_model": task.maker_model,
                "makerModel": task.maker_model,
                "steps": steps,
                "latest_seq": latest_seq,
                "latestSeq": latest_seq,
                "pending_followups": task.pending_followups,
                "pendingFollowups": task.pending_followups,
                "cancel_requested": task.cancel_requested,
                "cancelRequested": task.cancel_requested,
                "result_text": task.result_text,
                "resultText": task.result_text,
                "cards": task.cards,
                "approvals": task.approvals,
                "usage": task.usage,
                "created_at": task.created_at,
                "createdAt": task.created_at,
                "updated_at": task.updated_at,
                "updatedAt": task.updated_at,
            }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_rpc::dispatch_runtime_rpc;

    #[test]
    fn worker_ledger_followup_batching_reconnect_recovery_and_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path();

        let created = WorkerTaskStore::create_task(
            data_dir,
            "conv-ledger-1",
            "req-1",
            "Compile Quarterly Brief",
            "Summarize Q3 metrics and draft notes",
            "User prefers concise bullet points",
            "deepseek-v4.1-flash-1017",
            "deepseek-v4-pro",
        );
        assert_eq!(created.status, WorkerTaskStatus::Queued);
        assert_eq!(created.steps.len(), 1);
        assert_eq!(created.steps[0].kind, "dispatched");
        let seq_dispatched = created.steps[0].seq;

        let active = WorkerTaskStore::active_task_for_conversation(data_dir, "conv-ledger-1")
            .expect("queued task must be active for conversation");
        assert_eq!(active.task_id, created.task_id);

        WorkerTaskStore::mark_status(data_dir, &created.task_id, WorkerTaskStatus::Running);

        // Enqueue 3 rapid follow-ups directly to active worker
        let (_, c1) = WorkerTaskStore::enqueue_followup(
            data_dir,
            &created.task_id,
            "m1",
            "Include APAC revenue numbers",
        )
        .unwrap();
        let (_, c2) = WorkerTaskStore::enqueue_followup(
            data_dir,
            &created.task_id,
            "m2",
            "Exclude draft projections",
        )
        .unwrap();
        let (_, c3) = WorkerTaskStore::enqueue_followup(
            data_dir,
            &created.task_id,
            "m3",
            "Save final summary to notes",
        )
        .unwrap();
        assert_eq!((c1, c2, c3), (1, 2, 3));

        // Drain atomically batches all 3 and records followup_batched step
        let drained = WorkerTaskStore::drain_followups(data_dir, &created.task_id);
        assert_eq!(drained.len(), 3);
        assert!(WorkerTaskStore::drain_followups(data_dir, &created.task_id).is_empty());

        let step_tool = WorkerTaskStore::append_step(
            data_dir,
            &created.task_id,
            "tool_start",
            "Running note_save",
            Some(json!({ "name": "note_save" })),
        )
        .unwrap();

        WorkerTaskStore::complete_task(
            data_dir,
            &created.task_id,
            "Quarterly brief saved.",
            TurnUsageSummary {
                input_tokens: 100,
                output_tokens: 40,
                cache_tokens: 0,
                total_tokens: 140,
                cost_usd: 0.001,
            },
        );

        // Reconnect poll recovery via after_seq
        let delta_tasks =
            WorkerTaskStore::list_tasks(data_dir, Some("conv-ledger-1"), seq_dispatched);
        assert_eq!(delta_tasks.len(), 1);
        let recovered_steps = delta_tasks[0]["steps"].as_array().unwrap();
        assert_eq!(recovered_steps.len(), 3); // followup_batched, tool_start, completed
        assert_eq!(recovered_steps[0]["kind"], "followup_batched");
        assert_eq!(recovered_steps[1]["seq"], step_tool.seq);
        assert_eq!(recovered_steps[2]["kind"], "completed");

        // RPC query and cancel verification on a second task
        let second = WorkerTaskStore::create_task(
            data_dir,
            "conv-ledger-1",
            "req-2",
            "Second Background Job",
            "Long analysis",
            "",
            "gpt-4o-mini",
            "gpt-4o",
        );
        let rpc_tasks = dispatch_runtime_rpc(
            data_dir,
            "app_jarvis_worker_tasks",
            &json!({ "conversationId": "conv-ledger-1", "afterSeq": 0 }),
        )
        .unwrap();
        assert_eq!(rpc_tasks.as_array().unwrap().len(), 2);

        let cancelled = dispatch_runtime_rpc(
            data_dir,
            "app_jarvis_worker_cancel",
            &json!({ "taskId": second.task_id }),
        )
        .unwrap();
        assert_eq!(cancelled["status"], "cancelled");
        assert!(WorkerTaskStore::is_cancelled(data_dir, &second.task_id));
        assert!(
            WorkerTaskStore::active_task_for_conversation(data_dir, "conv-ledger-1").is_none()
        );
    }
}

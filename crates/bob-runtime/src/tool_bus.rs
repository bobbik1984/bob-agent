//! Bob Tool Bus & Graded Action Gates.
//!
//! Enforces:
//! - Graded action risk classification (`ReadOnly`, `SideEffect`, `Irreversible`, `SettingsChange`).
//! - Mode-aware gating (`Interactive` vs `UnattendedRoutine`).
//! - Structured card presentation (`StructuredCard`) for clear, readable UI presentation.
//! - Durable, atomic pending approvals store (`PendingApprovalStore`) with 10-minute expiry.
//! - Rejection tracking compatibility with `ToolCallTracker::record_rejected`.
//! - Standard core tool implementations backed by `bob_kernel::app::LocalApp`.
//! - Extensibility for custom dynamic tools (`register_custom_tool`).
//! - MCP tools list export for external harnesses (`export_mcp_tools_list`).

use bob_kernel::app::LocalApp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const APPROVAL_EXPIRY_MILLIS: i64 = 10 * 60 * 1000; // 10 minutes

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionRiskLevel {
    ReadOnly,
    SideEffect,
    Irreversible,
    SettingsChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolExecutionMode {
    Interactive,
    UnattendedRoutine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardItem {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredCard {
    pub id: String,
    pub kind: String, // "info" | "approval" | "takeover" | "routine_draft"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub title: String,
    pub summary: String,
    pub risk_level: ActionRiskLevel,
    #[serde(default)]
    pub fields: Vec<CardField>,
    #[serde(default)]
    pub items: Vec<CardItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingApprovalCard {
    pub id: String,
    pub tool_name: String,
    pub args: Value,
    pub card: StructuredCard,
    pub created_at: i64,
    pub expires_at: i64,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedActionDraft {
    pub id: String,
    pub tool_name: String,
    pub args: Value,
    pub card: StructuredCard,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolBusOutcome {
    Executed {
        output: Value,
        card: Option<StructuredCard>,
    },
    RequiresApproval {
        approval: PendingApprovalCard,
    },
    SavedAsDraft {
        draft: SavedActionDraft,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub risk_level: ActionRiskLevel,
}

pub type CustomToolFn =
    Arc<dyn Fn(&Value, &mut LocalApp) -> Result<Value, String> + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalStoreData {
    pub approvals: Vec<PendingApprovalCard>,
}

#[derive(Debug, Clone)]
pub struct PendingApprovalStore {
    file_path: PathBuf,
    pub data: ApprovalStoreData,
}

impl PendingApprovalStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("jarvis_approvals.json");

        let data = if file_path.is_file() {
            let content =
                std::fs::read_to_string(&file_path).map_err(|_| "APPROVALS_READ_FAILED")?;
            serde_json::from_str::<ApprovalStoreData>(&content).unwrap_or_default()
        } else {
            ApprovalStoreData::default()
        };

        let store = Self { file_path, data };
        let _ = store.save_atomic();
        Ok(store)
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "APPROVALS_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes()).map_err(|_| "APPROVALS_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "APPROVALS_WRITE_FAILED")?;
        Ok(())
    }

    pub fn insert(&mut self, card: PendingApprovalCard) -> Result<(), &'static str> {
        if let Some(pos) = self.data.approvals.iter().position(|a| a.id == card.id) {
            self.data.approvals[pos] = card;
        } else {
            self.data.approvals.push(card);
        }
        self.save_atomic()
    }

    pub fn get(&self, id: &str) -> Option<&PendingApprovalCard> {
        self.data.approvals.iter().find(|a| a.id == id)
    }

    pub fn list_pending(&mut self, now_ms: i64) -> Vec<PendingApprovalCard> {
        let mut modified = false;
        for a in &mut self.data.approvals {
            if a.status == ApprovalStatus::Pending && now_ms >= a.expires_at {
                a.status = ApprovalStatus::Expired;
                modified = true;
            }
        }
        if modified {
            let _ = self.save_atomic();
        }
        self.data
            .approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    pub fn approve_once(
        &mut self,
        approval_id: &str,
        bus: &BobToolBus,
        app: &mut LocalApp,
    ) -> Result<ToolBusOutcome, &'static str> {
        let now = crate::now_millis();
        let approval = self
            .data
            .approvals
            .iter_mut()
            .find(|a| a.id == approval_id)
            .ok_or("APPROVAL_NOT_FOUND")?;

        if approval.status != ApprovalStatus::Pending {
            return Err("APPROVAL_NOT_PENDING");
        }
        if now >= approval.expires_at {
            approval.status = ApprovalStatus::Expired;
            let _ = self.save_atomic();
            return Err("APPROVAL_EXPIRED");
        }

        let outcome = bus
            .execute_bypassing_approval(&approval.tool_name, &approval.args, app)
            .map_err(|_| "APPROVAL_EXECUTION_FAILED")?;

        approval.status = ApprovalStatus::Approved;
        let _ = self.save_atomic();
        Ok(outcome)
    }

    pub fn reject(&mut self, approval_id: &str) -> Result<(String, Value), &'static str> {
        let approval = self
            .data
            .approvals
            .iter_mut()
            .find(|a| a.id == approval_id)
            .ok_or("APPROVAL_NOT_FOUND")?;

        if approval.status != ApprovalStatus::Pending {
            return Err("APPROVAL_NOT_PENDING");
        }
        approval.status = ApprovalStatus::Rejected;
        let tool_name = approval.tool_name.clone();
        let args = approval.args.clone();
        let _ = self.save_atomic();
        Ok((tool_name, args))
    }
}

pub struct BobToolBus {
    specs: HashMap<String, ToolSpec>,
    custom_tools: HashMap<String, (ToolSpec, CustomToolFn)>,
    pub approval_store: Option<PendingApprovalStore>,
    pub data_dir: Option<PathBuf>,
}

impl Default for BobToolBus {
    fn default() -> Self {
        Self::new()
    }
}

impl BobToolBus {
    pub fn new() -> Self {
        let mut bus = Self {
            specs: HashMap::new(),
            custom_tools: HashMap::new(),
            approval_store: None,
            data_dir: None,
        };
        bus.register_builtin_tools();
        bus
    }

    pub fn with_approval_store(data_dir: &Path) -> Result<Self, &'static str> {
        let store = PendingApprovalStore::open(data_dir)?;
        let mut bus = Self::new();
        bus.approval_store = Some(store);
        bus.data_dir = Some(data_dir.to_path_buf());
        bus.register_feature_tools(data_dir);
        Ok(bus)
    }

    pub fn register_feature_tools(&mut self, data_dir: &Path) {
        let now = crate::now_millis();
        if let Ok(mut store) = crate::features::FeatureStore::open(data_dir) {
            let packages = store.list(now);
            for pkg in packages {
                if pkg.enabled && pkg.status != "disabled" {
                    for (route_name, spec) in pkg.routes {
                        let tool_name = format!("f_{}_{}", pkg.name, route_name);
                        let risk_level = if matches!(spec, crate::features::FeatureRouteSpec::StoragePatch { .. }) {
                            ActionRiskLevel::SideEffect
                        } else {
                            ActionRiskLevel::ReadOnly
                        };
                        let pkg_name_c = pkg.name.clone();
                        let route_name_c = route_name.clone();
                        let dir_buf = data_dir.to_path_buf();
                        let tool_spec = ToolSpec {
                            name: tool_name.clone(),
                            description: format!("Feature '{}' route '{}'", pkg.name, route_name),
                            input_schema: json!({ "type": "object", "properties": {} }),
                            risk_level,
                        };
                        let handler: CustomToolFn = Arc::new(move |args, _app| {
                            let call_args = json!({
                                "name": pkg_name_c,
                                "route": route_name_c,
                                "payload": args,
                            });
                            crate::features::execute_feature_call(&dir_buf, &call_args)
                                .map_err(|e| e.to_string())
                        });
                        self.register_custom_tool(tool_spec, handler);
                    }
                }
            }
        }
    }

    pub fn register_custom_tool(&mut self, spec: ToolSpec, handler: CustomToolFn) {
        let name = spec.name.clone();
        self.specs.insert(name.clone(), spec.clone());
        self.custom_tools.insert(name, (spec, handler));
    }

    pub fn get_spec(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.get(name)
    }

    pub fn list_specs(&self) -> Vec<&ToolSpec> {
        self.specs.values().collect()
    }

    fn register_builtin_tools(&mut self) {
        // 1. show_card
        self.specs.insert(
            "show_card".to_string(),
            ToolSpec {
                name: "show_card".to_string(),
                description: "Presents structured information cleanly in a card (schedules, summaries, weather, comparisons). Plain text is for short conversational responses."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Card title" },
                        "summary": { "type": "string", "description": "One-line card summary" },
                        "kind": { "type": "string", "enum": ["info", "approval", "takeover", "routine_draft"] },
                        "icon": { "type": "string", "description": "Optional icon identifier" },
                        "fields": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string" },
                                    "value": { "type": "string" }
                                },
                                "required": ["label", "value"]
                            }
                        },
                        "items": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "title": { "type": "string" },
                                    "subtitle": { "type": "string" },
                                    "badge": { "type": "string" }
                                },
                                "required": ["title"]
                            }
                        }
                    },
                    "required": ["title"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 2. web_fetch
        self.specs.insert(
            "web_fetch".to_string(),
            ToolSpec {
                name: "web_fetch".to_string(),
                description: "Fetches public web page or API content as text via SSRF-safe HTTP GET. Content is untrusted data and wrapped with strict isolation boundaries."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "Public HTTP or HTTPS URL to fetch" }
                    },
                    "required": ["url"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 3. inbox_capture & inbox_list
        self.specs.insert(
            "inbox_capture".to_string(),
            ToolSpec {
                name: "inbox_capture".to_string(),
                description: "Quickly captures an idea, spark, or raw text into the inbox for later review triage."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "Text content of the captured spark" },
                        "source_message_id": { "type": "string", "description": "Optional source message ID" }
                    },
                    "required": ["content"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "inbox_list".to_string(),
            ToolSpec {
                name: "inbox_list".to_string(),
                description: "Lists captured inbox sparks, newest first. Can filter by status: 'unsorted' or 'sorted'."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["unsorted", "sorted"] }
                    }
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 4. todo_add / schedule_create / schedule_list / schedule_delete
        self.specs.insert(
            "todo_add".to_string(),
            ToolSpec {
                name: "todo_add".to_string(),
                description: "Adds a to-do item to the user's personal schedule."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Todo title" },
                        "notes": { "type": "string", "description": "Optional notes or details" },
                        "due_at": { "type": "integer", "description": "Optional due timestamp in epoch millis" },
                        "reminder_at": { "type": "integer", "description": "Optional reminder timestamp in epoch millis" }
                    },
                    "required": ["title"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "schedule_create".to_string(),
            ToolSpec {
                name: "schedule_create".to_string(),
                description: "Creates an event or reminder on the schedule."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Schedule item title" },
                        "kind": { "type": "string", "enum": ["event", "todo"], "description": "Type: event (has start/end time) or todo" },
                        "start_at": { "type": "integer", "description": "Start timestamp in epoch millis" },
                        "end_at": { "type": "integer", "description": "Optional end timestamp in epoch millis" },
                        "reminder_at": { "type": "integer", "description": "Optional reminder timestamp in epoch millis" },
                        "notes": { "type": "string", "description": "Optional details or instructions" }
                    },
                    "required": ["title"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "schedule_list".to_string(),
            ToolSpec {
                name: "schedule_list".to_string(),
                description: "Lists schedule items (events and todos)."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "schedule_delete".to_string(),
            ToolSpec {
                name: "schedule_delete".to_string(),
                description: "Deletes a schedule item by ID. Has side effects and requires explicit confirmation in interactive mode."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Schedule item ID to delete" }
                    },
                    "required": ["id"]
                }),
                risk_level: ActionRiskLevel::SideEffect,
            },
        );

        // 5. note_save / note_list
        self.specs.insert(
            "note_save".to_string(),
            ToolSpec {
                name: "note_save".to_string(),
                description: "Saves or updates a markdown note in personal ops storage."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Optional note ID. Auto-generated if omitted" },
                        "title": { "type": "string", "description": "Note title" },
                        "body": { "type": "string", "description": "Note markdown content" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Optional tag labels" },
                        "project_id": { "type": "string", "description": "Optional project association" }
                    },
                    "required": ["title", "body"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "note_list".to_string(),
            ToolSpec {
                name: "note_list".to_string(),
                description: "Lists notes, optionally filtered by project_id."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "project_id": { "type": "string", "description": "Optional project filter" }
                    }
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 6. memory_add / memory_list / memory_forget
        self.specs.insert(
            "memory_add".to_string(),
            ToolSpec {
                name: "memory_add".to_string(),
                description: "Records a lasting memory about the user's standing preferences, context, or ongoing projects."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "body": { "type": "string", "description": "Memory statement" },
                        "supersedes_id": { "type": "string", "description": "Optional ID of superseded memory" }
                    },
                    "required": ["body"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "memory_list".to_string(),
            ToolSpec {
                name: "memory_list".to_string(),
                description: "Lists remembered user facts. Defaults to active memories."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "active_only": { "type": "boolean", "description": "If true, only active memories are returned (default true)" }
                    }
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "memory_forget".to_string(),
            ToolSpec {
                name: "memory_forget".to_string(),
                description: "Marks a memory as forgotten so it is no longer recalled."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Memory ID to forget" }
                    },
                    "required": ["id"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 7. ticket_save
        self.specs.insert(
            "ticket_save".to_string(),
            ToolSpec {
                name: "ticket_save".to_string(),
                description: "Saves a ticket or reservation with structured metadata and an optional schedule entry."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Optional ticket ID" },
                        "title": { "type": "string", "description": "Ticket title" },
                        "category": { "type": "string", "description": "Category (flight, train, hotel, event, etc.)" },
                        "ticket_meta": { "type": "object", "description": "Arbitrary structured booking details" },
                        "start_at": { "type": "integer", "description": "Optional start time in epoch millis" },
                        "end_at": { "type": "integer", "description": "Optional end time in epoch millis" },
                        "notes": { "type": "string", "description": "Optional summary notes" }
                    },
                    "required": ["title"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 8. dispatch_to_node_session
        self.specs.insert(
            "dispatch_to_node_session".to_string(),
            ToolSpec {
                name: "dispatch_to_node_session".to_string(),
                description: "Dispatches a subtask prompt to an external node session. Side effect: requires user approval before execution."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "connection_id": { "type": "string", "description": "Target connection or cluster node ID" },
                        "session_id": { "type": "string", "description": "Target session ID" },
                        "prompt": { "type": "string", "description": "Prompt instructions to execute on remote node" }
                    },
                    "required": ["connection_id", "session_id", "prompt"]
                }),
                risk_level: ActionRiskLevel::SideEffect,
            },
        );

        // 9. feature_create / feature_list / feature_rollback
        self.specs.insert(
            "feature_create".to_string(),
            ToolSpec {
                name: "feature_create".to_string(),
                description: "Installs or updates a sandboxed feature package in a 3-minute trial sandbox."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Unique package slug identifier (^[a-z0-9][a-z0-9_-]{0,47}$)" },
                        "title": { "type": "string", "description": "Human-readable package title" },
                        "description": { "type": "string", "description": "Feature package summary" },
                        "widget_html": { "type": "string", "description": "Optional self-contained HTML/CSS/JS widget" },
                        "routes": { "type": "object", "description": "Declarative pipeline route specifications" },
                        "tools": { "type": "array", "description": "Optional tool descriptors mapped to routes" },
                        "initial_storage": { "type": "object", "description": "Optional initial KV storage JSON (<= 8192 bytes)" }
                    },
                    "required": ["name", "title"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "feature_list".to_string(),
            ToolSpec {
                name: "feature_list".to_string(),
                description: "Lists all installed and trial feature packages."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "feature_rollback".to_string(),
            ToolSpec {
                name: "feature_rollback".to_string(),
                description: "Reverts a feature package to its immediately preceding version (or target version), or removes unconfirmed trial package."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Package slug identifier" },
                        "target_version": { "type": "integer", "description": "Optional specific version to restore" }
                    },
                    "required": ["name"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 10. skill_create / skill_list
        self.specs.insert(
            "skill_create".to_string(),
            ToolSpec {
                name: "skill_create".to_string(),
                description: "Creates or updates a reusable SOP / prompt skill injected into Bob's context."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Skill slug identifier" },
                        "title": { "type": "string", "description": "Skill title" },
                        "instructions": { "type": "string", "description": "SOP execution instructions (<= 4000 characters)" },
                        "triggers": { "type": "array", "items": { "type": "string" }, "description": "Keywords or trigger phrases" }
                    },
                    "required": ["name", "title", "instructions"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "skill_list".to_string(),
            ToolSpec {
                name: "skill_list".to_string(),
                description: "Lists configured custom skills and user SOPs."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 11. mcp_add / mcp_list
        self.specs.insert(
            "mcp_add".to_string(),
            ToolSpec {
                name: "mcp_add".to_string(),
                description: "Connects an external remote HTTPS MCP server. Changes settings and requires human approval card."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "MCP server identifier" },
                        "url": { "type": "string", "description": "Public HTTPS endpoint URL (strictly https://, private/loopback blocked)" },
                        "description": { "type": "string", "description": "Optional server description" }
                    },
                    "required": ["name", "url"]
                }),
                risk_level: ActionRiskLevel::SettingsChange,
            },
        );

        self.specs.insert(
            "mcp_list".to_string(),
            ToolSpec {
                name: "mcp_list".to_string(),
                description: "Lists configured remote MCP servers."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 12. routine_save / routine_trial_run / routine_enable / routine_list / run_report
        self.specs.insert(
            "routine_save".to_string(),
            ToolSpec {
                name: "routine_save".to_string(),
                description: "Saves a new or updated routine definition with recurrence and action steps. Defaults to disabled, requiring a trial run before enabling."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Optional routine ID" },
                        "title": { "type": "string", "description": "Routine title" },
                        "prompt_instruction": { "type": "string", "description": "Goal or instructions" },
                        "recurrence": { "type": "object", "description": "RecurrenceRule: Once, Daily, Weekly, or Monthly" },
                        "routine_steps": { "type": "array", "description": "Structured steps to execute" },
                        "utc_offset_minutes": { "type": "integer", "description": "Timezone offset in minutes (default 480 = UTC+8)" }
                    },
                    "required": ["title"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "routine_trial_run".to_string(),
            ToolSpec {
                name: "routine_trial_run".to_string(),
                description: "Executes a single in-app trial run of a routine in unattended mode. Produces draft action cards for side effects and unlocks automatic scheduling on success."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Routine ID" }
                    },
                    "required": ["id"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "routine_enable".to_string(),
            ToolSpec {
                name: "routine_enable".to_string(),
                description: "Enables or disables scheduled execution of a routine. Enabling enforces the mandatory trial run gate."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Routine ID" },
                        "enabled": { "type": "boolean", "description": "Whether routine schedule is enabled" }
                    },
                    "required": ["id", "enabled"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "routine_list".to_string(),
            ToolSpec {
                name: "routine_list".to_string(),
                description: "Lists all configured routines and pending unattended drafts."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        self.specs.insert(
            "run_report".to_string(),
            ToolSpec {
                name: "run_report".to_string(),
                description: "Structured self-check report emitted at the conclusion of a routine execution turn."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "goal_achieved": { "type": "boolean", "description": "Whether routine goal succeeded" },
                        "summary": { "type": "string", "description": "Summary of execution" },
                        "needs_reauth": { "type": "boolean", "description": "Whether interactive login/re-auth takeover is required" },
                        "error": { "type": "string", "description": "Optional error message" }
                    },
                    "required": ["goal_achieved", "summary"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 13. kb_search
        self.specs.insert(
            "kb_search".to_string(),
            ToolSpec {
                name: "kb_search".to_string(),
                description: "Searches the SQLite FTS5 knowledge base (notes, memories, decisions, captures, schedules) with ranked hits, snippets and entity links."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query or keywords" },
                        "limit": { "type": "integer", "description": "Max hits to return (default 5)" }
                    },
                    "required": ["query"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 14. morning_brief
        self.specs.insert(
            "morning_brief".to_string(),
            ToolSpec {
                name: "morning_brief".to_string(),
                description: "Generates today's structured Morning Brief showing schedule, pending approvals, unsorted sparks, and system highlights."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "date": { "type": "string", "description": "Optional date string YYYY-MM-DD" }
                    }
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 15. soul_update
        self.specs.insert(
            "soul_update".to_string(),
            ToolSpec {
                name: "soul_update".to_string(),
                description: "Updates Bob's persona soul prompt in SOUL.md. Requires human approval card."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "New SOUL.md persona content" }
                    },
                    "required": ["content"]
                }),
                risk_level: ActionRiskLevel::SettingsChange,
            },
        );

        // 16. host_list_files
        self.specs.insert(
            "host_list_files".to_string(),
            ToolSpec {
                name: "host_list_files".to_string(),
                description: "Lists directory entries inside an authorized workspace root directory."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "dir_path": { "type": "string", "description": "Path to directory within authorized workspace" },
                        "max_entries": { "type": "integer", "description": "Maximum entries to return (default 200)" }
                    },
                    "required": ["dir_path"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 17. host_read_file
        self.specs.insert(
            "host_read_file".to_string(),
            ToolSpec {
                name: "host_read_file".to_string(),
                description: "Reads a UTF-8 text file within an authorized workspace root (capped at 128 KB)."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Path to file within authorized workspace" },
                        "max_bytes": { "type": "integer", "description": "Maximum bytes to read (capped at 131,072)" }
                    },
                    "required": ["file_path"]
                }),
                risk_level: ActionRiskLevel::ReadOnly,
            },
        );

        // 18. host_write_file
        self.specs.insert(
            "host_write_file".to_string(),
            ToolSpec {
                name: "host_write_file".to_string(),
                description: "Writes content atomically to a file inside an authorized writable workspace root. Requires human approval."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Target file path within authorized workspace" },
                        "content": { "type": "string", "description": "File text content (capped at 512 KB)" }
                    },
                    "required": ["file_path", "content"]
                }),
                risk_level: ActionRiskLevel::SideEffect,
            },
        );

        // 19. host_run_command
        self.specs.insert(
            "host_run_command".to_string(),
            ToolSpec {
                name: "host_run_command".to_string(),
                description: "Executes a shell command bounded within an authorized writable workspace directory. Irreversible: strictly requires explicit human approval."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command string to execute" },
                        "cwd": { "type": "string", "description": "Optional working directory within authorized workspace" },
                        "timeout_ms": { "type": "integer", "description": "Execution timeout in milliseconds (default 15,000, max 60,000)" }
                    },
                    "required": ["command"]
                }),
                risk_level: ActionRiskLevel::Irreversible,
            },
        );
    }

    /// Evaluates graded action gates and executes or gates the tool call.
    pub fn execute(
        &mut self,
        tool_name: &str,
        args: &Value,
        mode: ToolExecutionMode,
        app: &mut LocalApp,
    ) -> Result<ToolBusOutcome, String> {
        let spec = self
            .specs
            .get(tool_name)
            .cloned()
            .ok_or_else(|| format!("TOOL_NOT_FOUND: {tool_name}"))?;

        match spec.risk_level {
            ActionRiskLevel::ReadOnly => self.execute_bypassing_approval(tool_name, args, app),
            ActionRiskLevel::SideEffect => match mode {
                ToolExecutionMode::Interactive => {
                    let now = crate::now_millis();
                    let approval_id = format!("appr-{}", now);
                    let card = self.create_approval_card(&approval_id, &spec, args);
                    let pending = PendingApprovalCard {
                        id: approval_id,
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                        card,
                        created_at: now,
                        expires_at: now + APPROVAL_EXPIRY_MILLIS,
                        status: ApprovalStatus::Pending,
                    };
                    if let Some(store) = self.approval_store.as_mut() {
                        let _ = store.insert(pending.clone());
                    }
                    Ok(ToolBusOutcome::RequiresApproval { approval: pending })
                }
                ToolExecutionMode::UnattendedRoutine => {
                    let now = crate::now_millis();
                    let draft_id = format!("draft-{}", now);
                    let card = self.create_routine_draft_card(&draft_id, &spec, args);
                    let draft = SavedActionDraft {
                        id: draft_id,
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                        card,
                        created_at: now,
                    };
                    Ok(ToolBusOutcome::SavedAsDraft { draft })
                }
            },
            ActionRiskLevel::Irreversible => match mode {
                ToolExecutionMode::Interactive => {
                    let now = crate::now_millis();
                    let approval_id = format!("appr-{}", now);
                    let card = self.create_approval_card(&approval_id, &spec, args);
                    let pending = PendingApprovalCard {
                        id: approval_id,
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                        card,
                        created_at: now,
                        expires_at: now + APPROVAL_EXPIRY_MILLIS,
                        status: ApprovalStatus::Pending,
                    };
                    if let Some(store) = self.approval_store.as_mut() {
                        let _ = store.insert(pending.clone());
                    }
                    Ok(ToolBusOutcome::RequiresApproval { approval: pending })
                }
                ToolExecutionMode::UnattendedRoutine => {
                    Err("IRREVERSIBLE_ACTION_BLOCKED_IN_ROUTINE".to_string())
                }
            },
            ActionRiskLevel::SettingsChange => match mode {
                ToolExecutionMode::Interactive => {
                    let now = crate::now_millis();
                    let approval_id = format!("appr-{}", now);
                    let card = self.create_approval_card(&approval_id, &spec, args);
                    let pending = PendingApprovalCard {
                        id: approval_id,
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                        card,
                        created_at: now,
                        expires_at: now + APPROVAL_EXPIRY_MILLIS,
                        status: ApprovalStatus::Pending,
                    };
                    if let Some(store) = self.approval_store.as_mut() {
                        let _ = store.insert(pending.clone());
                    }
                    Ok(ToolBusOutcome::RequiresApproval { approval: pending })
                }
                ToolExecutionMode::UnattendedRoutine => {
                    Err("SETTINGS_CHANGE_NOT_ALLOWED_IN_ROUTINE".to_string())
                }
            },
        }
    }

    /// Executes the underlying tool implementation directly, bypassing the risk gate.
    /// Used by `PendingApprovalStore::approve_once` after human confirmation.
    pub fn execute_bypassing_approval(
        &self,
        tool_name: &str,
        args: &Value,
        app: &mut LocalApp,
    ) -> Result<ToolBusOutcome, String> {
        let now = crate::now_millis();

        // Check custom tools first
        if let Some((_, handler)) = self.custom_tools.get(tool_name) {
            let output = handler(args, app)?;
            return Ok(ToolBusOutcome::Executed { output, card: None });
        }

        match tool_name {
            "show_card" => {
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Card")
                    .to_string();
                let summary = args
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let kind = args
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("info")
                    .to_string();
                let icon = args
                    .get("icon")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);

                let mut fields = Vec::new();
                if let Some(arr) = args.get("fields").and_then(Value::as_array) {
                    for f in arr {
                        if let (Some(l), Some(v)) = (
                            f.get("label").and_then(Value::as_str),
                            f.get("value").and_then(Value::as_str),
                        ) {
                            fields.push(CardField {
                                label: l.to_string(),
                                value: v.to_string(),
                            });
                        }
                    }
                }

                let mut items = Vec::new();
                if let Some(arr) = args.get("items").and_then(Value::as_array) {
                    for item in arr {
                        if let Some(t) = item.get("title").and_then(Value::as_str) {
                            items.push(CardItem {
                                title: t.to_string(),
                                subtitle: item
                                    .get("subtitle")
                                    .and_then(Value::as_str)
                                    .map(ToString::to_string),
                                badge: item
                                    .get("badge")
                                    .and_then(Value::as_str)
                                    .map(ToString::to_string),
                            });
                        }
                    }
                }

                let card_id = format!("card-{}", now);
                let card = StructuredCard {
                    id: card_id.clone(),
                    kind,
                    icon,
                    title,
                    summary,
                    risk_level: ActionRiskLevel::ReadOnly,
                    fields,
                    items,
                    status: Some("rendered".to_string()),
                    technical_detail: None,
                };
                Ok(ToolBusOutcome::Executed {
                    output: json!({ "status": "card_rendered", "card_id": card_id }),
                    card: Some(card),
                })
            }
            "web_fetch" => {
                let url = args
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_URL".to_string())?;

                let res = crate::net_guard::fetch_public_text(url, 128 * 1024, 15_000)
                    .map_err(|e| format!("WEB_FETCH_ERROR: {e}"))?;

                let bounded = format!(
                    "<<<BEGIN_UNTRUSTED_WEB_DATA url=\"{}\" status=\"{}\" bytes=\"{}\">>>\n{}\n<<<END_UNTRUSTED_WEB_DATA>>>",
                    res.url, res.status, res.bytes, res.text
                );

                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "url": res.url,
                        "status": res.status,
                        "content": bounded,
                        "bytes": res.bytes
                    }),
                    card: None,
                })
            }
            "inbox_capture" => {
                let content = args
                    .get("content")
                    .and_then(Value::as_str)
                    .or_else(|| args.get("content_ref").and_then(Value::as_str))
                    .ok_or_else(|| "MISSING_CONTENT".to_string())?;
                let source_msg = args.get("source_message_id").and_then(Value::as_str);

                let id = format!("cap-{}", now);
                let item = app
                    .create_capture(&id, source_msg, content, now)
                    .map_err(|e| format!("CAPTURE_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&item)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "inbox_list" => {
                let status = args.get("status").and_then(Value::as_str);
                let items = app
                    .list_inbox(status)
                    .map_err(|e| format!("INBOX_LIST_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&items)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "todo_add" => {
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_TITLE".to_string())?;
                let notes = args
                    .get("notes")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let start_at = args
                    .get("due_at")
                    .or_else(|| args.get("start_at"))
                    .and_then(Value::as_i64)
                    .unwrap_or(now);
                let reminder_at = args.get("reminder_at").and_then(Value::as_i64);

                let id = format!("sched-{}", now);
                let item = app
                    .create_schedule_item_with_kind(
                        &id,
                        None,
                        None,
                        title,
                        "todo",
                        start_at,
                        None,
                        reminder_at,
                        &notes,
                        now,
                    )
                    .map_err(|e| format!("SCHEDULE_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&item)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "schedule_create" => {
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_TITLE".to_string())?;
                let kind = args
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("event");
                let start_at = args
                    .get("start_at")
                    .and_then(Value::as_i64)
                    .unwrap_or(now);
                let end_at = args.get("end_at").and_then(Value::as_i64);
                let reminder_at = args.get("reminder_at").and_then(Value::as_i64);
                let notes = args
                    .get("notes")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let id = format!("sched-{}", now);
                let item = app
                    .create_schedule_item_with_kind(
                        &id,
                        None,
                        None,
                        title,
                        kind,
                        start_at,
                        end_at,
                        reminder_at,
                        &notes,
                        now,
                    )
                    .map_err(|e| format!("SCHEDULE_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&item)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "schedule_list" => {
                let items = app
                    .list_schedule_items()
                    .map_err(|e| format!("SCHEDULE_LIST_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&items)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "schedule_delete" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_ID".to_string())?;

                let item = app
                    .delete_schedule_item(id, now)
                    .map_err(|e| format!("SCHEDULE_DELETE_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&item)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "note_save" => {
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_TITLE".to_string())?;
                let body = args
                    .get("body")
                    .or_else(|| args.get("body_markdown"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("note-{}", now));
                let project_id = args.get("project_id").and_then(Value::as_str);

                let tags: Vec<String> = args
                    .get("tags")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default();

                let item = app
                    .upsert_note(&id, project_id, None, title, body, &tags, now)
                    .map_err(|e| format!("NOTE_SAVE_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&item)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "note_list" => {
                let project_id = args.get("project_id").and_then(Value::as_str);
                let items = app
                    .list_notes(project_id)
                    .map_err(|e| format!("NOTE_LIST_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&items)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "memory_add" => {
                let body = args
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_BODY".to_string())?;
                let supersedes_id = args.get("supersedes_id").and_then(Value::as_str);

                let id = format!("mem-{}", now);
                let _proposed = app
                    .propose_memory(&id, None, Some("agent_turn"), body, supersedes_id)
                    .map_err(|e| format!("MEMORY_PROPOSE_ERROR: {e:?}"))?;

                let active = app
                    .update_memory_state(&id, "active")
                    .map_err(|e| format!("MEMORY_ACTIVATE_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&active)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "memory_list" => {
                let active_only = args
                    .get("active_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let items = app
                    .list_memories(active_only)
                    .map_err(|e| format!("MEMORY_LIST_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&items)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "memory_forget" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_ID".to_string())?;

                let forgotten = app
                    .update_memory_state(id, "forgotten")
                    .map_err(|e| format!("MEMORY_FORGET_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: serde_json::to_value(&forgotten)
                        .map_err(|e| format!("SERIALIZATION_ERROR: {e}"))?,
                    card: None,
                })
            }
            "ticket_save" => {
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MISSING_TITLE".to_string())?;
                let category = args
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("general");
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("ticket-{}", now));
                let ticket_meta = args.get("ticket_meta").cloned().unwrap_or(json!({}));
                let start_at = args.get("start_at").and_then(Value::as_i64);
                let end_at = args.get("end_at").and_then(Value::as_i64);
                let notes = args
                    .get("notes")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let (note, sched) = app
                    .upsert_ticket(
                        &id,
                        title,
                        category,
                        &ticket_meta,
                        start_at,
                        end_at,
                        &notes,
                        now,
                    )
                    .map_err(|e| format!("TICKET_ERROR: {e:?}"))?;

                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "note": note,
                        "schedule": sched
                    }),
                    card: None,
                })
            }
            "dispatch_to_node_session" => {
                let conn_id = args
                    .get("connection_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let sess_id = args
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let prompt = args
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "status": "dispatched",
                        "connection_id": conn_id,
                        "session_id": sess_id,
                        "prompt": prompt
                    }),
                    card: None,
                })
            }
            "feature_create" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "FEATURE_NAME_REQUIRED".to_string())?;
                let title = args.get("title").and_then(Value::as_str).unwrap_or(name);
                let desc = args
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let widget_html = args
                    .get("widget_html")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let routes: HashMap<String, crate::features::FeatureRouteSpec> = args
                    .get("routes")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let tools: Vec<crate::features::FeatureToolSpec> = args
                    .get("tools")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let storage = args
                    .get("storage")
                    .or_else(|| args.get("initial_storage"))
                    .cloned();

                let mut store = crate::features::FeatureStore::open(dir)
                    .map_err(|e| format!("FEATURE_STORE_ERROR: {e}"))?;
                let feat = store
                    .create_or_update(
                        name,
                        title,
                        desc,
                        widget_html,
                        routes,
                        tools,
                        storage,
                        now,
                    )
                    .map_err(|e| format!("FEATURE_CREATE_FAILED: {e}"))?;

                let card = StructuredCard {
                    id: format!("card-feat-{}", now),
                    kind: "info".to_string(),
                    icon: Some("package".to_string()),
                    title: format!("Feature in Trial: {}", feat.title),
                    summary: format!(
                        "Feature '{}' installed into 3-minute trial sandbox. Click Keep to confirm or Undo to revert.",
                        feat.name
                    ),
                    risk_level: ActionRiskLevel::ReadOnly,
                    fields: vec![
                        CardField {
                            label: "Package".to_string(),
                            value: feat.name.clone(),
                        },
                        CardField {
                            label: "Version".to_string(),
                            value: format!("v{}", feat.version),
                        },
                        CardField {
                            label: "Trial Status".to_string(),
                            value: "Active (3-minute sandbox)".to_string(),
                        },
                    ],
                    items: vec![],
                    status: Some("trial".to_string()),
                    technical_detail: None,
                };

                Ok(ToolBusOutcome::Executed {
                    output: json!(feat),
                    card: Some(card),
                })
            }
            "feature_list" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let mut store = crate::features::FeatureStore::open(dir)
                    .map_err(|e| format!("FEATURE_STORE_ERROR: {e}"))?;
                let list = store.list(now);
                Ok(ToolBusOutcome::Executed {
                    output: json!(list),
                    card: None,
                })
            }
            "feature_rollback" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "FEATURE_NAME_REQUIRED".to_string())?;
                let target_version = args
                    .get("target_version")
                    .or_else(|| args.get("targetVersion"))
                    .and_then(Value::as_u64)
                    .map(|v| v as u32);
                let mut store = crate::features::FeatureStore::open(dir)
                    .map_err(|e| format!("FEATURE_STORE_ERROR: {e}"))?;
                let rolled = store
                    .rollback_feature(name, target_version, now)
                    .map_err(|e| format!("FEATURE_ROLLBACK_FAILED: {e}"))?;
                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "rolled_back": true,
                        "feature": rolled,
                    }),
                    card: None,
                })
            }
            "skill_create" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "SKILL_NAME_REQUIRED".to_string())?;
                let title = args.get("title").and_then(Value::as_str).unwrap_or(name);
                let instructions = args
                    .get("instructions")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "SKILL_INSTRUCTIONS_REQUIRED".to_string())?;
                let triggers: Vec<String> = args
                    .get("triggers")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut store = crate::features::SkillStore::open(dir)
                    .map_err(|e| format!("SKILL_STORE_ERROR: {e}"))?;
                let skill = store
                    .create_or_update(name, title, instructions, triggers, now)
                    .map_err(|e| format!("SKILL_CREATE_FAILED: {e}"))?;
                Ok(ToolBusOutcome::Executed {
                    output: json!(skill),
                    card: None,
                })
            }
            "skill_list" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let store = crate::features::SkillStore::open(dir)
                    .map_err(|e| format!("SKILL_STORE_ERROR: {e}"))?;
                let list = store.list();
                Ok(ToolBusOutcome::Executed {
                    output: json!(list),
                    card: None,
                })
            }
            "mcp_add" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MCP_NAME_REQUIRED".to_string())?;
                let url = args
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "MCP_URL_REQUIRED".to_string())?;
                let desc = args
                    .get("description")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let mut store = crate::features::McpStore::open(dir)
                    .map_err(|e| format!("MCP_STORE_ERROR: {e}"))?;
                let srv = store
                    .add_server(name, url, desc, now)
                    .map_err(|e| format!("MCP_ADD_FAILED: {e}"))?;
                Ok(ToolBusOutcome::Executed {
                    output: json!(srv),
                    card: None,
                })
            }
            "mcp_list" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let store = crate::features::McpStore::open(dir)
                    .map_err(|e| format!("MCP_STORE_ERROR: {e}"))?;
                let list = store.list();
                Ok(ToolBusOutcome::Executed {
                    output: json!(list),
                    card: None,
                })
            }
            "routine_save" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "ROUTINE_TITLE_REQUIRED".to_string())?;
                let prompt_instruction = args
                    .get("prompt_instruction")
                    .or_else(|| args.get("promptInstruction"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let recurrence: crate::routines::RecurrenceRule = if let Some(rec) = args.get("recurrence") {
                    serde_json::from_value(rec.clone())
                        .map_err(|e| format!("INVALID_RECURRENCE_RULE: {e}"))?
                } else {
                    crate::routines::RecurrenceRule::Daily { hour: 8, minute: 0 }
                };
                let steps: Vec<crate::routines::RoutineStep> = args
                    .get("routine_steps")
                    .or_else(|| args.get("routineSteps"))
                    .or_else(|| args.get("steps"))
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .enumerate()
                            .filter_map(|(idx, v)| {
                                let tool = v.get("tool_name").or_else(|| v.get("toolName")).and_then(Value::as_str)?;
                                let desc = v.get("description").and_then(Value::as_str).unwrap_or("");
                                let template = v.get("args_template").or_else(|| v.get("argsTemplate")).or_else(|| v.get("args")).cloned().unwrap_or(json!({}));
                                Some(crate::routines::RoutineStep {
                                    step_index: idx,
                                    tool_name: tool.to_string(),
                                    description: desc.to_string(),
                                    args_template: template,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let offset = args
                    .get("utc_offset_minutes")
                    .or_else(|| args.get("utcOffsetMinutes"))
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .unwrap_or(crate::routines::DEFAULT_UTC_OFFSET_MINUTES);
                let id = args
                    .get("id")
                    .and_then(Value::as_str);

                let routine = crate::routines::capture_walkthrough_to_routine(
                    id,
                    title,
                    prompt_instruction,
                    recurrence,
                    steps,
                    Some(offset),
                );
                let mut store = crate::routines::RoutineStore::open(dir)
                    .map_err(|e| format!("ROUTINES_STORE_ERROR: {e}"))?;
                let saved = store
                    .save_routine(routine)
                    .map_err(|e| format!("ROUTINE_SAVE_FAILED: {e}"))?;
                let card = StructuredCard {
                    id: format!("card-routine-{}", saved.id),
                    kind: "routine_draft".to_string(),
                    icon: Some("clock".to_string()),
                    title: format!("Routine Draft: {}", saved.title),
                    summary: "Routine authored as draft. A mandatory trial run is required before automatic scheduling can be enabled.".to_string(),
                    risk_level: ActionRiskLevel::ReadOnly,
                    fields: vec![
                        CardField {
                            label: "Recurrence".to_string(),
                            value: format!("{:?}", saved.recurrence),
                        },
                        CardField {
                            label: "Trial Status".to_string(),
                            value: "Trial Required".to_string(),
                        },
                    ],
                    items: vec![],
                    status: Some("pending_trial".to_string()),
                    technical_detail: Some(serde_json::to_string(&saved).unwrap_or_default()),
                };
                Ok(ToolBusOutcome::Executed {
                    output: json!(saved),
                    card: Some(card),
                })
            }
            "routine_trial_run" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let id = args
                    .get("id")
                    .or_else(|| args.get("routine_id"))
                    .or_else(|| args.get("routineId"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "ROUTINE_ID_REQUIRED".to_string())?;
                let mut store = crate::routines::RoutineStore::open(dir)
                    .map_err(|e| format!("ROUTINES_STORE_ERROR: {e}"))?;
                let mut draft_store = crate::routines::UnattendedDraftStore::open(dir)
                    .map_err(|e| format!("DRAFT_STORE_ERROR: {e}"))?;
                let mut bus = BobToolBus::with_approval_store(dir)
                    .map_err(|e| format!("BUS_INIT_ERROR: {e}"))?;
                let (routine, report, drafts) = store
                    .run_trial(id, &mut bus, &mut draft_store, app, now)
                    .map_err(|e| format!("TRIAL_RUN_FAILED: {e}"))?;
                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "routine": routine,
                        "report": report,
                        "drafts": drafts,
                    }),
                    card: None,
                })
            }
            "routine_enable" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let id = args
                    .get("id")
                    .or_else(|| args.get("routine_id"))
                    .or_else(|| args.get("routineId"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "ROUTINE_ID_REQUIRED".to_string())?;
                let enabled = args
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "ROUTINE_ENABLED_REQUIRED".to_string())?;
                let mut store = crate::routines::RoutineStore::open(dir)
                    .map_err(|e| format!("ROUTINES_STORE_ERROR: {e}"))?;
                let updated = store
                    .enable_routine(id, enabled, now)
                    .map_err(|e| e.to_string())?;
                Ok(ToolBusOutcome::Executed {
                    output: json!(updated),
                    card: None,
                })
            }
            "routine_list" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let store = crate::routines::RoutineStore::open(dir)
                    .map_err(|e| format!("ROUTINES_STORE_ERROR: {e}"))?;
                let mut draft_store = crate::routines::UnattendedDraftStore::open(dir)
                    .map_err(|e| format!("DRAFT_STORE_ERROR: {e}"))?;
                let routines = store.list();
                let drafts = draft_store.list_pending(now);
                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "routines": routines,
                        "drafts": drafts,
                    }),
                    card: None,
                })
            }
            "run_report" => {
                let goal_achieved = args
                    .get("goal_achieved")
                    .or_else(|| args.get("goalAchieved"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let summary = args
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let needs_reauth = args
                    .get("needs_reauth")
                    .or_else(|| args.get("needsReauth"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let error = args
                    .get("error")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "status": "recorded",
                        "goal_achieved": goal_achieved,
                        "summary": summary,
                        "needs_reauth": needs_reauth,
                        "error": error,
                    }),
                    card: None,
                })
            }
            "kb_search" => {
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "QUERY_REQUIRED".to_string())?;
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
                let hits = app
                    .kb_search(query, limit)
                    .map_err(|e| format!("KB_SEARCH_ERROR: {e:?}"))?;
                let mut items = Vec::new();
                for h in &hits {
                    items.push(CardItem {
                        title: format!("[{}] {}", h.doc_kind, h.title),
                        subtitle: Some(h.snippet.clone()),
                        badge: Some(h.doc_kind.clone()),
                    });
                }
                let card = if !hits.is_empty() {
                    Some(StructuredCard {
                        id: format!("card-kb-{}", now),
                        kind: "info".to_string(),
                        icon: Some("search".to_string()),
                        title: format!("Knowledge Search: \"{}\"", query),
                        summary: format!("Found {} matching entries in local knowledge base.", hits.len()),
                        risk_level: ActionRiskLevel::ReadOnly,
                        fields: vec![
                            CardField {
                                label: "Query".to_string(),
                                value: query.to_string(),
                            },
                            CardField {
                                label: "Hits".to_string(),
                                value: format!("{}", hits.len()),
                            },
                        ],
                        items,
                        status: Some("ready".to_string()),
                        technical_detail: None,
                    })
                } else {
                    None
                };
                Ok(ToolBusOutcome::Executed {
                    output: json!({
                        "hits": hits,
                        "count": hits.len()
                    }),
                    card,
                })
            }
            "morning_brief" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let brief = crate::growth::generate_morning_brief(dir, app, now)
                    .map_err(|e| format!("BRIEF_ERROR: {e}"))?;
                let card = brief.card.clone();
                Ok(ToolBusOutcome::Executed {
                    output: json!(brief),
                    card: Some(card),
                })
            }
            "soul_update" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let content = args
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "CONTENT_REQUIRED".to_string())?;
                let soul_path = dir.join("SOUL.md");
                std::fs::write(&soul_path, content.as_bytes())
                    .map_err(|e| format!("SOUL_WRITE_ERROR: {e}"))?;
                Ok(ToolBusOutcome::Executed {
                    output: json!({ "ok": true, "path": soul_path.display().to_string() }),
                    card: None,
                })
            }
            "host_list_files" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let dir_path = args
                    .get("dir_path")
                    .or_else(|| args.get("path"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "DIR_PATH_REQUIRED".to_string())?;
                let max_entries = args
                    .get("max_entries")
                    .or_else(|| args.get("maxEntries"))
                    .and_then(Value::as_u64)
                    .map(|v| v as usize);
                let res = crate::host_tools::host_list_files(dir, dir_path, max_entries)?;
                Ok(ToolBusOutcome::Executed {
                    output: res,
                    card: None,
                })
            }
            "host_read_file" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let file_path = args
                    .get("file_path")
                    .or_else(|| args.get("path"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "FILE_PATH_REQUIRED".to_string())?;
                let max_bytes = args
                    .get("max_bytes")
                    .or_else(|| args.get("maxBytes"))
                    .and_then(Value::as_u64)
                    .map(|v| v as usize);
                let res = crate::host_tools::host_read_file(dir, file_path, max_bytes)?;
                Ok(ToolBusOutcome::Executed {
                    output: res,
                    card: None,
                })
            }
            "host_write_file" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let file_path = args
                    .get("file_path")
                    .or_else(|| args.get("path"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "FILE_PATH_REQUIRED".to_string())?;
                let content = args
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "CONTENT_REQUIRED".to_string())?;
                let res = crate::host_tools::host_write_file(dir, file_path, content)?;
                Ok(ToolBusOutcome::Executed {
                    output: res,
                    card: None,
                })
            }
            "host_run_command" => {
                let dir = self.data_dir.as_deref().unwrap_or_else(|| Path::new("."));
                let command = args
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "COMMAND_REQUIRED".to_string())?;
                let cwd = args
                    .get("cwd")
                    .or_else(|| args.get("working_dir"))
                    .and_then(Value::as_str);
                let timeout_ms = args
                    .get("timeout_ms")
                    .or_else(|| args.get("timeoutMs"))
                    .and_then(Value::as_u64);
                let res = crate::host_tools::host_run_command(dir, command, cwd, timeout_ms)?;
                Ok(ToolBusOutcome::Executed {
                    output: res,
                    card: None,
                })
            }
            other => Err(format!("UNKNOWN_TOOL: {other}")),
        }
    }

    fn create_approval_card(
        &self,
        approval_id: &str,
        spec: &ToolSpec,
        args: &Value,
    ) -> StructuredCard {
        let title = match spec.name.as_str() {
            "schedule_delete" => "Confirm Schedule Deletion".to_string(),
            "dispatch_to_node_session" => "Confirm Remote Node Task Dispatch".to_string(),
            "mcp_add" => "Confirm External Remote MCP Server Addition".to_string(),
            "soul_update" => "Confirm SOUL Persona Evolution".to_string(),
            "host_write_file" => "Confirm Host File Write".to_string(),
            "host_run_command" => "Confirm Host Command Execution".to_string(),
            other => format!("Approve Action: {}", other),
        };
        let summary = match spec.name.as_str() {
            "schedule_delete" => {
                let id = args.get("id").and_then(Value::as_str).unwrap_or("unknown");
                format!("Requesting confirmation to delete schedule item '{}'", id)
            }
            "dispatch_to_node_session" => {
                let conn = args
                    .get("connection_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let sess = args
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                format!(
                    "Requesting authorization to dispatch prompt to node '{}' (session '{}')",
                    conn, sess
                )
            }
            "mcp_add" => {
                let name = args.get("name").and_then(Value::as_str).unwrap_or("unknown");
                let url = args.get("url").and_then(Value::as_str).unwrap_or("");
                format!(
                    "Requesting authorization to connect external remote HTTPS MCP server '{}' at '{}'",
                    name, url
                )
            }
            "soul_update" => {
                "Requesting confirmation to evolve Bob's core persona in SOUL.md based on recent interaction insights.".to_string()
            }
            "host_write_file" => {
                let p = args.get("file_path").or_else(|| args.get("path")).and_then(Value::as_str).unwrap_or("unknown");
                let len = args.get("content").and_then(Value::as_str).map(|s| s.len()).unwrap_or(0);
                format!("Requesting authorization to write file '{}' ({} bytes)", p, len)
            }
            "host_run_command" => {
                let cmd = args.get("command").and_then(Value::as_str).unwrap_or("");
                let cwd = args.get("cwd").and_then(Value::as_str).unwrap_or("workspace");
                format!("Requesting authorization to execute command '{}' in '{}'", cmd, cwd)
            }
            _ => spec.description.clone(),
        };

        let mut fields = Vec::new();
        if let Value::Object(map) = args {
            for (k, v) in map {
                fields.push(CardField {
                    label: k.clone(),
                    value: match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    },
                });
            }
        }

        StructuredCard {
            id: approval_id.to_string(),
            kind: "approval".to_string(),
            icon: Some("shield-alert".to_string()),
            title,
            summary,
            risk_level: spec.risk_level,
            fields,
            items: Vec::new(),
            status: Some("pending".to_string()),
            technical_detail: Some(serde_json::to_string(args).unwrap_or_default()),
        }
    }

    fn create_routine_draft_card(
        &self,
        draft_id: &str,
        spec: &ToolSpec,
        args: &Value,
    ) -> StructuredCard {
        let mut card = self.create_approval_card(draft_id, spec, args);
        card.kind = "routine_draft".to_string();
        card.icon = Some("file-text".to_string());
        card.status = Some("saved_as_draft".to_string());
        card
    }

    pub fn approve_once(
        &mut self,
        approval_id: &str,
        app: &mut LocalApp,
    ) -> Result<ToolBusOutcome, &'static str> {
        let approval = self
            .approval_store
            .as_ref()
            .and_then(|s| s.get(approval_id))
            .cloned()
            .ok_or("APPROVAL_NOT_FOUND")?;

        let now = crate::now_millis();
        if approval.status != ApprovalStatus::Pending {
            return Err("APPROVAL_NOT_PENDING");
        }
        if now >= approval.expires_at {
            if let Some(store) = self.approval_store.as_mut() {
                if let Some(a) = store.data.approvals.iter_mut().find(|a| a.id == approval_id) {
                    a.status = ApprovalStatus::Expired;
                    let _ = store.save_atomic();
                }
            }
            return Err("APPROVAL_EXPIRED");
        }

        let outcome = self
            .execute_bypassing_approval(&approval.tool_name, &approval.args, app)
            .map_err(|_| "APPROVAL_EXECUTION_FAILED")?;

        if let Some(store) = self.approval_store.as_mut() {
            if let Some(a) = store.data.approvals.iter_mut().find(|a| a.id == approval_id) {
                a.status = ApprovalStatus::Approved;
                let _ = store.save_atomic();
            }
        }

        Ok(outcome)
    }

    pub fn reject(&mut self, approval_id: &str) -> Result<(String, Value), &'static str> {
        let store = self.approval_store.as_mut().ok_or("NO_APPROVAL_STORE")?;
        store.reject(approval_id)
    }

    /// Exposes tools list in standard MCP format: `{"tools": [{"name": "...", "description": "...", "inputSchema": {...}}]}`.
    pub fn export_mcp_tools_list(&self) -> Value {
        let mut list = Vec::new();
        let mut sorted_specs: Vec<&ToolSpec> = self.specs.values().collect();
        sorted_specs.sort_by(|a, b| a.name.cmp(&b.name));

        for spec in sorted_specs {
            list.push(json!({
                "name": spec.name,
                "description": spec.description,
                "inputSchema": spec.input_schema,
            }));
        }
        json!({ "tools": list })
    }

    /// Exposes tools in OpenAI tool calling format.
    pub fn export_openai_tools(&self) -> Vec<Value> {
        let mut list = Vec::new();
        let mut sorted_specs: Vec<&ToolSpec> = self.specs.values().collect();
        sorted_specs.sort_by(|a, b| a.name.cmp(&b.name));

        for spec in sorted_specs {
            list.push(json!({
                "type": "function",
                "function": {
                    "name": spec.name,
                    "description": spec.description,
                    "parameters": spec.input_schema
                }
            }));
        }
        list
    }

    /// Exposes tools in Anthropic messages tool calling format.
    pub fn export_anthropic_tools(&self) -> Vec<Value> {
        let mut list = Vec::new();
        let mut sorted_specs: Vec<&ToolSpec> = self.specs.values().collect();
        sorted_specs.sort_by(|a, b| a.name.cmp(&b.name));

        for spec in sorted_specs {
            list.push(json!({
                "name": spec.name,
                "description": spec.description,
                "input_schema": spec.input_schema
            }));
        }
        list
    }

    /// Exposes tools in Gemini generateContent tool calling format.
    pub fn export_gemini_tools(&self) -> Value {
        let mut decls = Vec::new();
        let mut sorted_specs: Vec<&ToolSpec> = self.specs.values().collect();
        sorted_specs.sort_by(|a, b| a.name.cmp(&b.name));

        for spec in sorted_specs {
            decls.push(json!({
                "name": spec.name,
                "description": spec.description,
                "parameters": spec.input_schema
            }));
        }
        json!({ "functionDeclarations": decls })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_app(dir: &Path) -> LocalApp {
        let db_path = dir.join("test_app.db");
        LocalApp::open(&db_path).expect("LocalApp::open should succeed")
    }

    #[test]
    fn test_core_tools_registration_and_mcp_export() {
        let bus = BobToolBus::new();
        assert!(bus.get_spec("show_card").is_some());
        assert!(bus.get_spec("web_fetch").is_some());
        assert!(bus.get_spec("todo_add").is_some());
        assert!(bus.get_spec("schedule_create").is_some());
        assert!(bus.get_spec("schedule_delete").is_some());
        assert!(bus.get_spec("dispatch_to_node_session").is_some());

        let mcp = bus.export_mcp_tools_list();
        let tools = mcp.get("tools").and_then(Value::as_array).unwrap();
        assert!(tools.len() >= 8);

        let openai_tools = bus.export_openai_tools();
        assert!(openai_tools.iter().any(|t| t["function"]["name"] == "todo_add"));

        let anthropic_tools = bus.export_anthropic_tools();
        assert!(anthropic_tools.iter().any(|t| t["name"] == "todo_add"));

        let gemini_tools = bus.export_gemini_tools();
        assert!(gemini_tools["functionDeclarations"].as_array().unwrap().len() >= 8);
    }

    #[test]
    fn test_show_card_generates_structured_card() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut bus = BobToolBus::new();

        let args = json!({
            "title": "Beijing Weather",
            "summary": "Clear, 18C, light breeze",
            "fields": [
                { "label": "Temp", "value": "18C" },
                { "label": "Humidity", "value": "45%" }
            ],
            "items": [
                { "title": "Tomorrow", "subtitle": "Sunny", "badge": "High 20C" }
            ]
        });

        let outcome = bus
            .execute("show_card", &args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();

        match outcome {
            ToolBusOutcome::Executed { output, card } => {
                assert_eq!(output["status"], "card_rendered");
                let card = card.expect("show_card should emit StructuredCard");
                assert_eq!(card.title, "Beijing Weather");
                assert_eq!(card.fields.len(), 2);
                assert_eq!(card.items.len(), 1);
            }
            _ => panic!("Expected Executed outcome for show_card"),
        }
    }

    #[test]
    fn test_graded_action_gates_interactive_pause_and_resume() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();

        // 1. Create a schedule item first
        let create_args = json!({
            "title": "Doctor Appointment",
            "notes": "Bring medical card"
        });
        let outcome = bus
            .execute("todo_add", &create_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();
        let created_id = match outcome {
            ToolBusOutcome::Executed { output, .. } => output["id"].as_str().unwrap().to_string(),
            _ => panic!("Expected Executed for todo_add"),
        };

        // 2. Calling schedule_delete (SideEffect) in Interactive mode must pause with RequiresApproval
        let del_args = json!({ "id": created_id });
        let outcome = bus
            .execute("schedule_delete", &del_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();

        let approval_id = match outcome {
            ToolBusOutcome::RequiresApproval { approval } => {
                assert_eq!(approval.tool_name, "schedule_delete");
                assert_eq!(approval.status, ApprovalStatus::Pending);
                assert_eq!(approval.card.kind, "approval");
                approval.id
            }
            _ => panic!("Expected RequiresApproval for schedule_delete in Interactive mode"),
        };

        // Verify stored in approval store
        let pending = bus.approval_store.as_mut().unwrap().list_pending(crate::now_millis());
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, approval_id);

        // 3. Approve once executes and deletes the item
        let approved_outcome = bus.approve_once(&approval_id, &mut app).unwrap();
        match approved_outcome {
            ToolBusOutcome::Executed { output, .. } => {
                assert_eq!(output["id"], created_id);
            }
            _ => panic!("Expected Executed outcome after approve_once"),
        }

        // Verify the schedule item was really deleted from LocalApp
        let remaining = app.list_schedule_items().unwrap();
        assert!(!remaining.iter().any(|s| s.id == created_id));
    }

    #[test]
    fn test_unattended_routine_saves_draft_for_side_effect() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();

        let del_args = json!({ "id": "sched-12345" });
        let outcome = bus
            .execute("schedule_delete", &del_args, ToolExecutionMode::UnattendedRoutine, &mut app)
            .unwrap();

        match outcome {
            ToolBusOutcome::SavedAsDraft { draft } => {
                assert_eq!(draft.tool_name, "schedule_delete");
                assert_eq!(draft.card.kind, "routine_draft");
            }
            _ => panic!("Expected SavedAsDraft for SideEffect in UnattendedRoutine mode"),
        }
    }

    #[test]
    fn test_approval_rejection_returns_tool_and_args() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();

        let dispatch_args = json!({
            "connection_id": "conn-vps-1",
            "session_id": "sess-main",
            "prompt": "Run cargo check"
        });

        let outcome = bus
            .execute("dispatch_to_node_session", &dispatch_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();

        let approval_id = match outcome {
            ToolBusOutcome::RequiresApproval { approval } => approval.id,
            _ => panic!("Expected RequiresApproval"),
        };

        let (tool, args) = bus.reject(&approval_id).unwrap();
        assert_eq!(tool, "dispatch_to_node_session");
        assert_eq!(args["connection_id"], "conn-vps-1");

        // Trying to approve a rejected action must fail
        assert_eq!(
            bus.approve_once(&approval_id, &mut app).unwrap_err(),
            "APPROVAL_NOT_PENDING"
        );
    }

    #[test]
    fn test_routine_tools_save_trial_enable_flow() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = create_test_app(dir.path());
        let mut bus = BobToolBus::with_approval_store(dir.path()).unwrap();

        // 1. routine_save
        let save_args = json!({
            "id": "rtn-bus-test",
            "title": "Daily Backup",
            "prompt_instruction": "Perform daily backup",
            "recurrence": { "type": "daily", "hour": 2, "minute": 0 },
            "routine_steps": [
                { "tool_name": "inbox_capture", "description": "Capture backup note", "args": { "content": "Backup started" } }
            ]
        });
        let outcome = bus
            .execute("routine_save", &save_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();
        match outcome {
            ToolBusOutcome::Executed { output, card } => {
                assert_eq!(output["id"], "rtn-bus-test");
                let card = card.expect("card must be present");
                assert_eq!(card.kind, "routine_draft");
            }
            _ => panic!("Expected Executed for routine_save"),
        }

        // 2. routine_enable before trial must fail with ROUTINE_TRIAL_REQUIRED
        let enable_args = json!({ "id": "rtn-bus-test", "enabled": true });
        let err = bus
            .execute("routine_enable", &enable_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap_err();
        assert_eq!(err, "ROUTINE_TRIAL_REQUIRED");

        // 3. routine_trial_run
        let trial_args = json!({ "id": "rtn-bus-test" });
        let outcome = bus
            .execute("routine_trial_run", &trial_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();
        match outcome {
            ToolBusOutcome::Executed { output, .. } => {
                assert_eq!(output["routine"]["trial_passed"], true);
                assert_eq!(output["report"]["goal_achieved"], true);
            }
            _ => panic!("Expected Executed for routine_trial_run"),
        }

        // 4. routine_enable after trial succeeds
        let outcome = bus
            .execute("routine_enable", &enable_args, ToolExecutionMode::Interactive, &mut app)
            .unwrap();
        match outcome {
            ToolBusOutcome::Executed { output, .. } => {
                assert_eq!(output["enabled"], true);
            }
            _ => panic!("Expected Executed for routine_enable"),
        }

        // 5. routine_list
        let outcome = bus
            .execute("routine_list", &json!({}), ToolExecutionMode::Interactive, &mut app)
            .unwrap();
        match outcome {
            ToolBusOutcome::Executed { output, .. } => {
                let routines = output["routines"].as_array().unwrap();
                assert_eq!(routines.len(), 1);
            }
            _ => panic!("Expected Executed for routine_list"),
        }
    }
}

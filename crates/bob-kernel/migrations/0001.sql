PRAGMA foreign_keys = ON;
CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at INTEGER NOT NULL);
CREATE TABLE devices (
  id TEXT PRIMARY KEY, label TEXT NOT NULL, public_key_ref TEXT,
  trust_state TEXT NOT NULL CHECK(trust_state IN ('unpaired','paired','revoked')),
  created_at INTEGER NOT NULL
);
CREATE TABLE home_authority (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1), device_id TEXT NOT NULL REFERENCES devices(id),
  epoch INTEGER NOT NULL CHECK(epoch>0)
);
CREATE TABLE projects (
  id TEXT PRIMARY KEY, title TEXT NOT NULL,
  acceptance TEXT NOT NULL DEFAULT 'home' CHECK(acceptance IN ('local','home','rejected')),
  status TEXT NOT NULL CHECK(status IN ('active','archived')),
  version INTEGER NOT NULL CHECK(version>0), created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE workspaces (
  id TEXT PRIMARY KEY, device_id TEXT NOT NULL REFERENCES devices(id),
  directory_ref TEXT NOT NULL, policy_ref TEXT NOT NULL, version INTEGER NOT NULL CHECK(version>0),
  UNIQUE(device_id,directory_ref)
);
CREATE TABLE project_workspaces (
  project_id TEXT NOT NULL REFERENCES projects(id), workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  PRIMARY KEY(project_id,workspace_id)
);
CREATE TABLE conversations (
  id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id), title TEXT NOT NULL,
  acceptance TEXT NOT NULL DEFAULT 'home' CHECK(acceptance IN ('local','home','rejected')),
  version INTEGER NOT NULL CHECK(version>0), created_at INTEGER NOT NULL, archived_at INTEGER
);
CREATE TABLE native_sessions (
  id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id),
  device_id TEXT NOT NULL REFERENCES devices(id), workspace_id TEXT REFERENCES workspaces(id),
  service_instance_id TEXT NOT NULL, harness TEXT NOT NULL, native_session_id TEXT NOT NULL,
  capability_json TEXT NOT NULL CHECK(json_valid(capability_json)),
  UNIQUE(device_id,service_instance_id,harness,native_session_id)
);
CREATE TABLE messages (
  id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id),
  origin_device_id TEXT NOT NULL REFERENCES devices(id), request_id TEXT UNIQUE,
  role TEXT NOT NULL CHECK(role IN ('user','assistant','tool','system')),
  content_json TEXT NOT NULL CHECK(json_valid(content_json)),
  acceptance TEXT NOT NULL CHECK(acceptance IN ('local','home','rejected')),
  created_at INTEGER NOT NULL
);
CREATE TABLE captures (
  id TEXT PRIMARY KEY, origin_device_id TEXT NOT NULL REFERENCES devices(id),
  source_message_id TEXT REFERENCES messages(id), content_ref TEXT NOT NULL, content_sha256 TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE memories (
  id TEXT PRIMARY KEY, source_capture_id TEXT REFERENCES captures(id), source_message_id TEXT REFERENCES messages(id),
  body TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('proposed','active','superseded','forgotten')),
  supersedes_id TEXT REFERENCES memories(id), version INTEGER NOT NULL CHECK(version>0),
  CHECK(source_capture_id IS NOT NULL OR source_message_id IS NOT NULL)
);
CREATE TABLE goals (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), title TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('active','paused','achieved','cancelled')),
  version INTEGER NOT NULL CHECK(version>0), UNIQUE(project_id,id)
);
CREATE TABLE milestones (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, goal_id TEXT NOT NULL, title TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('planned','active','achieved','cancelled')),
  version INTEGER NOT NULL CHECK(version>0), UNIQUE(project_id,goal_id,id),
  FOREIGN KEY(project_id,goal_id) REFERENCES goals(project_id,id)
);
CREATE TABLE tasks (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), goal_id TEXT, milestone_id TEXT,
  source_message_id TEXT REFERENCES messages(id), title TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('draft','queued','running','blocked','awaiting_review','changes_requested','completed','cancelled')),
  constraints_json TEXT NOT NULL CHECK(json_valid(constraints_json)), version INTEGER NOT NULL CHECK(version>0),
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(project_id,id),
  CHECK(milestone_id IS NULL OR goal_id IS NOT NULL),
  FOREIGN KEY(project_id,goal_id) REFERENCES goals(project_id,id),
  FOREIGN KEY(project_id,goal_id,milestone_id) REFERENCES milestones(project_id,goal_id,id)
);
CREATE TABLE decisions (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), task_id TEXT,
  source_message_id TEXT NOT NULL REFERENCES messages(id), body TEXT NOT NULL,
  rationale_json TEXT NOT NULL CHECK(json_valid(rationale_json)),
  status TEXT NOT NULL CHECK(status IN ('proposed','accepted','superseded','rejected')),
  supersedes_id TEXT REFERENCES decisions(id), version INTEGER NOT NULL CHECK(version>0),
  FOREIGN KEY(project_id,task_id) REFERENCES tasks(project_id,id)
);
CREATE TABLE executions (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, task_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL, native_session_binding_id TEXT REFERENCES native_sessions(id),
  request_id TEXT NOT NULL UNIQUE, parent_execution_id TEXT, parent_artifact_version_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('pending','dispatching','running','paused','completed','failed','cancelled','unknown')),
  requested_json TEXT NOT NULL CHECK(json_valid(requested_json)),
  resolved_json TEXT CHECK(resolved_json IS NULL OR json_valid(resolved_json)),
  observed_json TEXT CHECK(observed_json IS NULL OR json_valid(observed_json)),
  observed_evidence_ref TEXT, contract_json TEXT NOT NULL CHECK(json_valid(contract_json)), contract_sha256 TEXT NOT NULL,
  stop_requested_at INTEGER, stop_confirmed_at INTEGER, started_at INTEGER, ended_at INTEGER,
  version INTEGER NOT NULL CHECK(version>0), UNIQUE(task_id,id), UNIQUE(workspace_id,id),
  CHECK(observed_json IS NULL OR observed_evidence_ref IS NOT NULL),
  CHECK(stop_confirmed_at IS NULL OR stop_requested_at IS NOT NULL),
  CHECK(parent_artifact_version_id IS NULL OR parent_execution_id IS NOT NULL),
  FOREIGN KEY(project_id,task_id) REFERENCES tasks(project_id,id),
  FOREIGN KEY(project_id,workspace_id) REFERENCES project_workspaces(project_id,workspace_id),
  FOREIGN KEY(task_id,parent_execution_id) REFERENCES executions(task_id,id),
  FOREIGN KEY(task_id,parent_execution_id,parent_artifact_version_id) REFERENCES artifact_versions(task_id,execution_id,id)
);
CREATE UNIQUE INDEX one_live_execution_per_task ON executions(task_id)
  WHERE status IN ('pending','dispatching','running','paused','unknown');
CREATE TRIGGER immutable_execution_contract BEFORE UPDATE ON executions
WHEN NEW.request_id!=OLD.request_id OR NEW.task_id!=OLD.task_id OR NEW.project_id!=OLD.project_id
  OR NEW.workspace_id!=OLD.workspace_id OR NEW.requested_json!=OLD.requested_json
  OR NEW.contract_json!=OLD.contract_json OR NEW.contract_sha256!=OLD.contract_sha256
  OR NEW.parent_execution_id IS NOT OLD.parent_execution_id
  OR NEW.parent_artifact_version_id IS NOT OLD.parent_artifact_version_id
  OR (OLD.resolved_json IS NOT NULL AND NEW.resolved_json IS NOT OLD.resolved_json)
BEGIN SELECT RAISE(ABORT,'EXECUTION_CONTRACT_IMMUTABLE'); END;
CREATE TABLE execution_observations (
  id TEXT PRIMARY KEY, execution_id TEXT NOT NULL REFERENCES executions(id),
  field TEXT NOT NULL, value_json TEXT NOT NULL CHECK(json_valid(value_json)),
  basis TEXT NOT NULL CHECK(basis IN ('provider_reported','dispatch_observed','fixture')),
  evidence_ref TEXT NOT NULL, observed_at INTEGER NOT NULL
);
CREATE TABLE workspace_leases (
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id), execution_id TEXT NOT NULL UNIQUE,
  home_epoch INTEGER NOT NULL, fencing_token INTEGER NOT NULL CHECK(fencing_token>0),
  state TEXT NOT NULL CHECK(state IN ('held','stop_pending','unknown','released')),
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(workspace_id,execution_id) REFERENCES executions(workspace_id,id)
);
CREATE TABLE artifacts (
  id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES tasks(id), title TEXT NOT NULL,
  kind TEXT NOT NULL, UNIQUE(task_id,id)
);
CREATE TABLE artifact_versions (
  id TEXT PRIMARY KEY, artifact_id TEXT NOT NULL, task_id TEXT NOT NULL, execution_id TEXT NOT NULL,
  parent_version_id TEXT, ordinal INTEGER NOT NULL CHECK(ordinal>0),
  manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json)), manifest_sha256 TEXT,
  snapshot_ref TEXT, snapshot_state TEXT NOT NULL CHECK(snapshot_state IN ('available','missing')),
  created_at INTEGER NOT NULL, UNIQUE(artifact_id,ordinal), UNIQUE(artifact_id,id), UNIQUE(task_id,execution_id,id),
  CHECK(snapshot_state!='available' OR (snapshot_ref IS NOT NULL AND manifest_sha256 IS NOT NULL)),
  FOREIGN KEY(task_id,artifact_id) REFERENCES artifacts(task_id,id),
  FOREIGN KEY(task_id,execution_id) REFERENCES executions(task_id,id),
  FOREIGN KEY(artifact_id,parent_version_id) REFERENCES artifact_versions(artifact_id,id)
);
CREATE TRIGGER immutable_artifact_version_update BEFORE UPDATE ON artifact_versions
BEGIN SELECT RAISE(ABORT,'ARTIFACT_VERSION_IMMUTABLE'); END;
CREATE TRIGGER immutable_artifact_version_delete BEFORE DELETE ON artifact_versions
BEGIN SELECT RAISE(ABORT,'ARTIFACT_VERSION_IMMUTABLE'); END;
CREATE TABLE verification_runs (
  id TEXT PRIMARY KEY, task_id TEXT NOT NULL, execution_id TEXT NOT NULL, artifact_version_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('pending','running','passed','failed','blocked','not_run')),
  scope_json TEXT NOT NULL CHECK(json_valid(scope_json)), verifier TEXT NOT NULL,
  started_at INTEGER, ended_at INTEGER, UNIQUE(task_id,execution_id,id),
  FOREIGN KEY(task_id,execution_id) REFERENCES executions(task_id,id),
  FOREIGN KEY(task_id,execution_id,artifact_version_id) REFERENCES artifact_versions(task_id,execution_id,id)
);
CREATE TABLE verification_checks (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES verification_runs(id), check_key TEXT NOT NULL,
  required INTEGER NOT NULL DEFAULT 1 CHECK(required IN (0,1)),
  result TEXT NOT NULL CHECK(result IN ('passed','failed','blocked','not_run','not_applicable')),
  evidence_ref TEXT, explanation TEXT NOT NULL, UNIQUE(run_id,check_key),
  CHECK(result NOT IN ('passed','failed') OR evidence_ref IS NOT NULL)
);
CREATE TABLE reviews (
  id TEXT PRIMARY KEY, task_id TEXT NOT NULL, execution_id TEXT NOT NULL, artifact_version_id TEXT NOT NULL,
  verification_run_id TEXT NOT NULL, actor_device_id TEXT NOT NULL REFERENCES devices(id),
  verdict TEXT NOT NULL CHECK(verdict IN ('accepted','changes_requested','rejected')),
  request_id TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL,
  FOREIGN KEY(task_id,execution_id,artifact_version_id) REFERENCES artifact_versions(task_id,execution_id,id),
  FOREIGN KEY(task_id,execution_id,verification_run_id) REFERENCES verification_runs(task_id,execution_id,id)
);
CREATE TABLE evidence (
  id TEXT PRIMARY KEY, source_kind TEXT NOT NULL, source_ref TEXT NOT NULL,
  content_sha256 TEXT NOT NULL, captured_at INTEGER NOT NULL,
  verification_scope TEXT NOT NULL
);
CREATE TABLE evidence_links (
  evidence_id TEXT NOT NULL REFERENCES evidence(id), object_kind TEXT NOT NULL, object_id TEXT NOT NULL,
  relation TEXT NOT NULL, PRIMARY KEY(evidence_id,object_kind,object_id,relation)
);
CREATE TABLE legacy_refs (
  source_namespace TEXT NOT NULL, source_id TEXT NOT NULL, source_revision TEXT NOT NULL,
  object_kind TEXT NOT NULL, object_id TEXT NOT NULL, source_sha256 TEXT NOT NULL,
  PRIMARY KEY(source_namespace,source_id,source_revision)
);
CREATE TABLE domain_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE,
  authority_epoch INTEGER NOT NULL, object_kind TEXT NOT NULL, object_id TEXT NOT NULL,
  object_version INTEGER NOT NULL, request_id TEXT, source_device_id TEXT NOT NULL REFERENCES devices(id),
  type TEXT NOT NULL, payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), created_at INTEGER NOT NULL,
  UNIQUE(authority_epoch,object_kind,object_id,object_version)
);
CREATE TABLE replica_cursors (
  home_device_id TEXT PRIMARY KEY REFERENCES devices(id), authority_epoch INTEGER NOT NULL,
  last_seq INTEGER NOT NULL CHECK(last_seq>=0)
);
CREATE TABLE intent_outbox (
  request_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES conversations(id),
  message_id TEXT UNIQUE REFERENCES messages(id), origin_device_id TEXT NOT NULL REFERENCES devices(id),
  target_device_id TEXT NOT NULL REFERENCES devices(id), home_epoch INTEGER NOT NULL,
  native_session_binding_id TEXT REFERENCES native_sessions(id),
  kind TEXT NOT NULL CHECK(kind IN ('append_message','capture','create_task','task_update','execute','cancel_execution','answer_permission','review')),
  lane TEXT NOT NULL DEFAULT 'normal' CHECK(lane IN ('normal','control')),
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), payload_sha256 TEXT NOT NULL,
  expected_version INTEGER CHECK(expected_version>=0), permission_ref TEXT,
  send_state TEXT NOT NULL CHECK(send_state IN ('local_saved','queued_for_wakeup','not_dispatched','dispatching','dispatched','unknown_send','peer_accepted','rejected','cancelled_local','needs_attention')),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0), wake_attempts INTEGER NOT NULL DEFAULT 0 CHECK(wake_attempts>=0),
  last_error TEXT, created_at INTEGER NOT NULL, dispatched_at INTEGER, acked_at INTEGER,
  next_attempt_at INTEGER, expires_at INTEGER, lease_owner TEXT, lease_until INTEGER,
  transport_evidence TEXT NOT NULL CHECK(transport_evidence IN ('none','confirmed_zero','started_or_possible','peer_receipt')),
  CHECK(send_state NOT IN ('dispatched','unknown_send') OR transport_evidence='started_or_possible'),
  CHECK(send_state!='peer_accepted' OR (acked_at IS NOT NULL AND transport_evidence='peer_receipt')),
  CHECK(kind NOT IN ('cancel_execution','answer_permission') OR lane='control'),
  CHECK(kind IN ('append_message','capture') OR expected_version IS NOT NULL)
);
CREATE INDEX outbox_due ON intent_outbox(send_state,next_attempt_at,created_at);
CREATE TABLE inbox_receipts (
  request_id TEXT PRIMARY KEY, origin_device_id TEXT NOT NULL REFERENCES devices(id),
  home_epoch INTEGER NOT NULL, payload_sha256 TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK(outcome IN ('accepted','rejected')),
  result_json TEXT NOT NULL CHECK(json_valid(result_json)), effect_ref TEXT, decided_at INTEGER NOT NULL
);
CREATE TABLE runner_dispatches (
  execution_id TEXT PRIMARY KEY REFERENCES executions(id), request_id TEXT NOT NULL UNIQUE,
  runner_instance_id TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('prepared','dispatching','accepted','rejected','unknown','terminal')),
  native_receipt_ref TEXT, last_error TEXT, updated_at INTEGER NOT NULL
);

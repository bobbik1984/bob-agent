CREATE TABLE conversation_view_states(
  origin_device_id TEXT NOT NULL REFERENCES devices(id),
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  draft TEXT NOT NULL DEFAULT '',
  draft_revision INTEGER NOT NULL DEFAULT 0 CHECK(draft_revision >= 0),
  scroll_anchor_message_id TEXT,
  scroll_offset INTEGER NOT NULL DEFAULT 0,
  last_read_message_id TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(origin_device_id, conversation_id)
);

CREATE TABLE device_ui_preferences(
  origin_device_id TEXT PRIMARY KEY REFERENCES devices(id),
  selected_conversation_id TEXT REFERENCES conversations(id),
  sidebar_group_mode TEXT NOT NULL DEFAULT 'project' CHECK(sidebar_group_mode IN ('project','device')),
  locale TEXT NOT NULL DEFAULT 'zh-CN' CHECK(locale IN ('zh-CN','en-US')),
  theme TEXT NOT NULL DEFAULT 'system' CHECK(theme IN ('system','light','dark')),
  updated_at INTEGER NOT NULL
);

CREATE TABLE home_device_session_policies(
  device_id TEXT PRIMARY KEY REFERENCES devices(id),
  expires_at INTEGER NOT NULL,
  allow_cancel INTEGER NOT NULL CHECK(allow_cancel IN (0,1))
);

INSERT OR IGNORE INTO home_device_session_policies(device_id, expires_at, allow_cancel)
SELECT g.device_id, MAX(g.expires_at), MAX(g.allow_cancel)
FROM home_conversation_grants g
JOIN home_device_authorizations a ON a.device_id = g.device_id
WHERE a.revoked = 0
GROUP BY g.device_id;

UPDATE conversations SET project_id = NULL WHERE project_id LIKE 'local-project:%';

CREATE TABLE workspace_states(
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
  availability_state TEXT NOT NULL DEFAULT 'online' CHECK(availability_state IN ('online','sleeping','offline')),
  availability_reason TEXT NOT NULL DEFAULT '',
  head_commit TEXT NOT NULL DEFAULT '',
  branch TEXT NOT NULL DEFAULT 'main',
  upstream_delta_ahead INTEGER NOT NULL DEFAULT 0,
  upstream_delta_behind INTEGER NOT NULL DEFAULT 0,
  consistency_state TEXT NOT NULL DEFAULT 'synced' CHECK(consistency_state IN ('synced','ahead','behind','diverged','unverified')),
  work_state TEXT NOT NULL DEFAULT 'clean' CHECK(work_state IN ('clean','dirty_uncommitted','stashed','patch_bundle_pending','sync_failed')),
  dirty_summary_json TEXT NOT NULL DEFAULT '{}',
  missing_assets_json TEXT NOT NULL DEFAULT '[]',
  updated_at INTEGER NOT NULL
);

CREATE TABLE workspace_handoffs(
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  conversation_id TEXT REFERENCES conversations(id),
  source_workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  target_workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  checkpoint_kind TEXT NOT NULL CHECK(checkpoint_kind IN ('git_commit_pushed','patch_bundle_pushed')),
  base_commit TEXT NOT NULL,
  patch_bundle_sha256 TEXT,
  dirty_manifest_sha256 TEXT NOT NULL,
  required_assets_json TEXT NOT NULL DEFAULT '[]',
  state TEXT NOT NULL CHECK(state IN ('checkpoint_created','target_verified','completed','blocked')),
  blocked_reason TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE online_provider_sessions(
  id TEXT PRIMARY KEY,
  provider_node_id TEXT NOT NULL,
  provider_label TEXT NOT NULL,
  harness TEXT NOT NULL,
  native_session_id TEXT NOT NULL,
  project_path TEXT NOT NULL,
  title TEXT NOT NULL,
  summary_snippet TEXT NOT NULL DEFAULT '',
  turn_count INTEGER NOT NULL DEFAULT 0,
  write_active INTEGER NOT NULL DEFAULT 0 CHECK(write_active IN (0,1)),
  resumable INTEGER NOT NULL DEFAULT 0 CHECK(resumable IN (0,1)),
  bound_conversation_id TEXT REFERENCES conversations(id),
  continuation_mode TEXT CHECK(continuation_mode IN ('bounded_history_context','native_resume_verified')),
  updated_at INTEGER NOT NULL,
  UNIQUE(provider_node_id, harness, native_session_id)
);

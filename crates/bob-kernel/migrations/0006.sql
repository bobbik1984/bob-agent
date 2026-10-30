CREATE TABLE direct_provider_profiles(
  id TEXT PRIMARY KEY,
  provider_kind TEXT NOT NULL CHECK(provider_kind IN ('openai_compatible','anthropic_messages','gemini_generate_content')),
  label TEXT NOT NULL,
  base_url TEXT NOT NULL,
  model_id TEXT NOT NULL,
  api_key_slot TEXT NOT NULL DEFAULT 'default',
  api_key_configured INTEGER NOT NULL DEFAULT 0 CHECK(api_key_configured IN (0,1)),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
  last_check_status TEXT NOT NULL DEFAULT 'untested' CHECK(last_check_status IN ('untested','ok','auth_failed','network_error','rate_limited','model_not_found','invalid_config')),
  last_check_reason TEXT NOT NULL DEFAULT '',
  last_checked_at INTEGER,
  updated_at INTEGER NOT NULL
);

CREATE TABLE conversation_execution_routes(
  conversation_id TEXT PRIMARY KEY REFERENCES conversations(id),
  execution_route TEXT NOT NULL DEFAULT 'auto' CHECK(execution_route IN ('auto','direct_byok','local_cli','remote_home','online_node')),
  updated_at INTEGER NOT NULL
);

CREATE TABLE replica_direct_sync_outbox(
  sync_request_id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL UNIQUE,
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  user_message_id TEXT NOT NULL REFERENCES messages(id),
  execution_id TEXT NOT NULL REFERENCES executions(id),
  reply_message_id TEXT NOT NULL REFERENCES messages(id),
  origin_device_id TEXT NOT NULL REFERENCES devices(id),
  home_device_id TEXT NOT NULL REFERENCES devices(id),
  home_epoch INTEGER NOT NULL CHECK(home_epoch > 0),
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256) = 64),
  sync_state TEXT NOT NULL DEFAULT 'pending' CHECK(sync_state IN ('pending','synced','rejected')),
  last_error TEXT,
  created_at INTEGER NOT NULL,
  synced_at INTEGER
);

CREATE TABLE notes(
  id TEXT PRIMARY KEY,
  project_id TEXT REFERENCES projects(id),
  conversation_id TEXT REFERENCES conversations(id),
  origin_device_id TEXT NOT NULL REFERENCES devices(id),
  title TEXT NOT NULL,
  body_markdown TEXT NOT NULL,
  tags_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags_json)),
  acceptance TEXT NOT NULL DEFAULT 'home' CHECK(acceptance IN ('local','home','rejected')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE schedule_items(
  id TEXT PRIMARY KEY,
  project_id TEXT REFERENCES projects(id),
  task_id TEXT REFERENCES tasks(id),
  origin_device_id TEXT NOT NULL REFERENCES devices(id),
  title TEXT NOT NULL,
  start_at INTEGER NOT NULL,
  end_at INTEGER,
  reminder_at INTEGER,
  status TEXT NOT NULL DEFAULT 'scheduled' CHECK(status IN ('scheduled','completed','cancelled')),
  notes TEXT NOT NULL DEFAULT '',
  acceptance TEXT NOT NULL DEFAULT 'home' CHECK(acceptance IN ('local','home','rejected')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

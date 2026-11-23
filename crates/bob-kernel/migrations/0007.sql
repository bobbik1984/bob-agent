ALTER TABLE schedule_items ADD COLUMN kind TEXT NOT NULL DEFAULT 'event' CHECK(kind IN ('event','todo'));

CREATE TABLE personal_ops_sync_outbox(
  sync_id TEXT PRIMARY KEY,
  entity_kind TEXT NOT NULL CHECK(entity_kind IN ('capture','note','schedule','memory','decision')),
  entity_id TEXT NOT NULL,
  op_kind TEXT NOT NULL CHECK(op_kind IN ('upsert','status_change')),
  origin_device_id TEXT NOT NULL REFERENCES devices(id),
  home_device_id TEXT NOT NULL REFERENCES devices(id),
  home_epoch INTEGER NOT NULL CHECK(home_epoch > 0),
  expected_version INTEGER NOT NULL DEFAULT 0 CHECK(expected_version >= 0),
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256) = 64),
  sync_state TEXT NOT NULL DEFAULT 'pending' CHECK(sync_state IN ('pending','synced','conflict','rejected')),
  last_error TEXT,
  created_at INTEGER NOT NULL,
  synced_at INTEGER
);

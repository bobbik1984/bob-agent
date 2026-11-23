-- BOB-D-014 One Inbox: triage state of a capture ("spark").
-- Capture content stays immutable in `captures` (its digest is the sync identity);
-- a capture without a row here is unsorted. Sorted sparks are never deleted and keep
-- a link to where they went.
CREATE TABLE capture_triage (
 capture_id TEXT PRIMARY KEY REFERENCES captures(id),
 triage_status TEXT NOT NULL CHECK(triage_status IN ('unsorted','sorted')),
 destination_kind TEXT CHECK(destination_kind IN ('project_todo','calendar','note','merged')),
 destination_ref_id TEXT,
 destination_label TEXT,
 triage_batch_id TEXT,
 version INTEGER NOT NULL CHECK(version > 0),
 updated_at INTEGER NOT NULL,
 CHECK((triage_status = 'sorted') = (destination_kind IS NOT NULL))
);
CREATE INDEX capture_triage_status ON capture_triage(triage_status);

-- Replica → Home outbox must accept triage ops, and the 'delete' ops that note/schedule
-- deletion already enqueues (0007 omitted it, so deleting on a paired replica failed).
-- SQLite cannot alter a CHECK constraint: rebuild with identical columns and copy rows.
CREATE TABLE personal_ops_sync_outbox_v11(
  sync_id TEXT PRIMARY KEY,
  entity_kind TEXT NOT NULL CHECK(entity_kind IN ('capture','note','schedule','memory','decision','capture_triage')),
  entity_id TEXT NOT NULL,
  op_kind TEXT NOT NULL CHECK(op_kind IN ('upsert','status_change','delete')),
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
INSERT INTO personal_ops_sync_outbox_v11 SELECT * FROM personal_ops_sync_outbox;
DROP TABLE personal_ops_sync_outbox;
ALTER TABLE personal_ops_sync_outbox_v11 RENAME TO personal_ops_sync_outbox;

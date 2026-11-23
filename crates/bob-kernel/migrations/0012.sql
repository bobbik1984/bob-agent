-- BOB-D-014 Step 2: persisted Daily Review drafts. The payload is the runtime's validated
-- suggestion list (opaque to the kernel). Drafts never write to destinations by themselves;
-- only user confirmation does (Step 4). Local to the instance that generated it.
CREATE TABLE triage_batches (
 id TEXT PRIMARY KEY,
 status TEXT NOT NULL CHECK(status IN ('draft','applied','dismissed')),
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 model_ref TEXT,
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);
CREATE INDEX triage_batches_status_created ON triage_batches(status, created_at);

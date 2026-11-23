-- Association/transport evidence only; Task/Execution retain their lifecycle.
CREATE TABLE assistant_dispatch_links (
 request_id TEXT PRIMARY KEY REFERENCES intent_outbox(request_id),
 connection_id TEXT NOT NULL,
 native_session_id TEXT NOT NULL,
 target_title TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('unknown','accepted','completed','failed','ambiguous')),
 result_sequences_json TEXT NOT NULL DEFAULT '[]',
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);

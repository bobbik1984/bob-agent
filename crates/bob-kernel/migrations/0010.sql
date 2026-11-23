-- Local-owner links only: no copy of remote history or implicit write handoff.
CREATE TABLE project_session_links (
 connection_id TEXT NOT NULL,
 harness TEXT NOT NULL,
 native_session_id TEXT NOT NULL,
 project_id TEXT REFERENCES projects(id),
 owner_instance TEXT NOT NULL,
 version INTEGER NOT NULL CHECK(version > 0),
 updated_at INTEGER NOT NULL,
 PRIMARY KEY(connection_id,harness,native_session_id)
);

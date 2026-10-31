CREATE TABLE capture_media (
 id TEXT PRIMARY KEY, mime TEXT NOT NULL, bytes BLOB NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE capture_media_links (
 capture_id TEXT PRIMARY KEY REFERENCES captures(id), media_id TEXT NOT NULL REFERENCES capture_media(id)
);

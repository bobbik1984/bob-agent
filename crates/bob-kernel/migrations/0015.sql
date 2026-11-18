ALTER TABLE capture_triage ADD COLUMN resolution_status TEXT NOT NULL DEFAULT 'pending' CHECK(resolution_status IN ('pending','implemented','wont_do'));
ALTER TABLE capture_triage ADD COLUMN resolution_note TEXT;
ALTER TABLE capture_triage ADD COLUMN resolved_at INTEGER;

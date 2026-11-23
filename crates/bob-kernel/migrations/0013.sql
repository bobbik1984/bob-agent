CREATE TABLE bot_todo_receipts (
 actor TEXT NOT NULL, request_id TEXT NOT NULL, payload_sha256 TEXT NOT NULL,
 response_json TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(actor,request_id)
);

-- Durable operation grants. No private keys or transport credentials.
CREATE TABLE home_device_authorizations (
 device_id TEXT PRIMARY KEY REFERENCES devices(id),
 home_device_id TEXT NOT NULL REFERENCES devices(id), home_epoch INTEGER NOT NULL CHECK(home_epoch>0),
 paired_key_ref TEXT NOT NULL,
 identity_sha256 TEXT NOT NULL UNIQUE CHECK(length(identity_sha256)=64),
 revision INTEGER NOT NULL CHECK(revision>0), revoked INTEGER NOT NULL CHECK(revoked IN (0,1)),
 updated_at INTEGER NOT NULL
);
CREATE TABLE home_conversation_grants (
 device_id TEXT NOT NULL REFERENCES home_device_authorizations(device_id), conversation_id TEXT NOT NULL,
 expires_at INTEGER NOT NULL, allow_cancel INTEGER NOT NULL CHECK(allow_cancel IN (0,1)),
 PRIMARY KEY(device_id,conversation_id)
);

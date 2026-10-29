-- Pairing metadata only: no invitation secret or private key.
CREATE TABLE home_pairing_invitations (
 id TEXT PRIMARY KEY, home_device_id TEXT NOT NULL REFERENCES devices(id),
 home_epoch INTEGER NOT NULL CHECK(home_epoch>0), home_identity_sha256 TEXT NOT NULL,
 challenge_sha256 TEXT NOT NULL CHECK(length(challenge_sha256)=64),
 created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('open','pending','consumed','cancelled')),
 device_id TEXT, identity_sha256 TEXT, public_key_ref TEXT, confirmed_at INTEGER,
 CHECK(expires_at>created_at)
);

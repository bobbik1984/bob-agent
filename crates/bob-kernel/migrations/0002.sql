CREATE TABLE app_local_identity (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 origin_device_id TEXT NOT NULL REFERENCES devices(id),
 default_session_id TEXT NOT NULL UNIQUE
);

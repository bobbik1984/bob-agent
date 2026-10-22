//! Independent local vault: no sync export, no fixed password, no plaintext credential file.
//! AES-256-GCM with an OS-wrapped random key. Argon2 is legacy migration only.
use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use argon2::Argon2;
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const STORAGE: &str = "REMOTE_STORAGE";
const LOCKED: &str = "REMOTE_VAULT_LOCKED";
const SYSTEM_PREFIX: &[u8] = b"BOB-SYSTEM-VAULT-v1:";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub adapter: String,
    pub session_id: String,
    pub auto_reconnect: bool,
    pub uncertain_sessions: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Entry { profile: Profile, cookie: String }

pub struct Vault { path: PathBuf, key: [u8; 32], salt: Vec<u8>, entries: Vec<Entry> }
impl Drop for Vault { fn drop(&mut self) { self.key.fill(0); } }

fn database(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|_| STORAGE)?;
    conn.execute_batch("PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS vault(id INTEGER PRIMARY KEY CHECK(id=1), salt BLOB NOT NULL, nonce BLOB NOT NULL, ciphertext BLOB NOT NULL);")
        .map_err(|_| STORAGE)?;
    Ok(conn)
}
impl Vault {
    pub fn unlock_system<F>(path: PathBuf, legacy_password: Option<&str>, protect: F) -> Result<Self, String>
    where F: FnOnce(Option<&[u8]>) -> Result<super::system_protection::ProtectedKey, String> {
        use rusqlite::OptionalExtension;
        let conn = database(&path)?;
        let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> = conn.query_row(
            "SELECT salt,nonce,ciphertext FROM vault WHERE id=1", [],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|_| STORAGE)?;
        drop(conn);
        if let Some((salt, _, _)) = &row {
            if !salt.starts_with(SYSTEM_PREFIX) {
                if salt.len() != 16 { return Err(STORAGE.into()); }
                // Legacy data stays intact until the new OS-protected snapshot commits.
                let password = legacy_password.ok_or("REMOTE_VAULT_MIGRATION_REQUIRED")?;
                let mut legacy = Self::unlock(path.clone(), password)?;
                let protected = protect(None)?;
                let mut vault = Self { path, key: protected.key, salt: [SYSTEM_PREFIX, &protected.envelope].concat(), entries: vec![] };
                vault.commit(std::mem::take(&mut legacy.entries))?;
                return Ok(vault);
            }
        }
        let envelope = row.as_ref().map(|(salt,_,_)| &salt[SYSTEM_PREFIX.len()..]);
        let protected = protect(envelope)?;
        let mut vault = Self { path, key: protected.key, salt: [SYSTEM_PREFIX, &protected.envelope].concat(), entries: vec![] };
        if let Some((_, nonce, ciphertext)) = row {
            if nonce.len() != 12 || ciphertext.len() > 4*1024*1024 { return Err(STORAGE.into()); }
            let mut plain = Aes256Gcm::new_from_slice(&vault.key).map_err(|_| STORAGE)?
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref()).map_err(|_| "REMOTE_SYSTEM_KEY_UNAVAILABLE")?;
            let decoded = serde_json::from_slice(&plain).map_err(|_| STORAGE);
            plain.fill(0);
            vault.entries = decoded?;
        } else { vault.commit(vec![])?; }
        Ok(vault)
    }
    // Used only to migrate an existing pre-release password vault and in legacy tests.
    pub fn unlock(path: PathBuf, password: &str) -> Result<Self, String> {
        if password.len() < 12 || password.len() > 1024 { return Err("REMOTE_VAULT_PASSWORD".into()); }
        let conn = database(&path)?;
        use rusqlite::OptionalExtension;
        let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> = conn.query_row(
            "SELECT salt, nonce, ciphertext FROM vault WHERE id=1", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().map_err(|_| STORAGE)?;
        let salt = match &row {
            Some((salt, _, _)) if salt.len() == 16 => salt.clone(),
            Some(_) => return Err(STORAGE.into()),
            None => { let mut salt = vec![0;16]; rand::rngs::OsRng.fill_bytes(&mut salt); salt }
        };
        let mut key = [0;32];
        Argon2::default().hash_password_into(password.as_bytes(), &salt, &mut key).map_err(|_| STORAGE)?;
        let mut vault = Self { path, key, salt, entries: vec![] };
        if let Some((_, nonce, ciphertext)) = row {
            if nonce.len() != 12 || ciphertext.len() > 4*1024*1024 { return Err(STORAGE.into()); }
            let plain = Aes256Gcm::new_from_slice(&vault.key).map_err(|_| STORAGE)?
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref()).map_err(|_| "REMOTE_VAULT_PASSWORD")?;
            vault.entries = serde_json::from_slice(&plain).map_err(|_| STORAGE)?;
        } else { vault.commit(vec![])?; }
        Ok(vault)
    }
    fn commit(&mut self, entries: Vec<Entry>) -> Result<(), String> {
        let mut plain = serde_json::to_vec(&entries).map_err(|_| STORAGE)?;
        let mut nonce = [0;12]; rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = Aes256Gcm::new_from_slice(&self.key).map_err(|_| STORAGE)?
            .encrypt(Nonce::from_slice(&nonce), plain.as_ref()).map_err(|_| STORAGE)?;
        plain.fill(0);
        let mut conn = database(&self.path)?;
        let tx = conn.transaction().map_err(|_| STORAGE)?;
        tx.execute("INSERT INTO vault VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET salt=excluded.salt, nonce=excluded.nonce, ciphertext=excluded.ciphertext",
            params![self.salt, nonce.as_slice(), ciphertext]).map_err(|_| STORAGE)?;
        tx.commit().map_err(|_| STORAGE)?;
        self.entries = entries;
        Ok(())
    }
    pub fn list(&self) -> Vec<Profile> { self.entries.iter().map(|e| e.profile.clone()).collect() }
    pub fn credential(&self, id: &str) -> Result<(Profile, String), String> {
        self.entries.iter().find(|e| e.profile.id == id).map(|e| (e.profile.clone(), e.cookie.clone())).ok_or_else(|| "REMOTE_NOT_FOUND".into())
    }
    pub fn save(&mut self, name: String, endpoint: String, cookie: String) -> Result<Profile, String> {
        if name.trim().is_empty() || name.len() > 120 { return Err("REMOTE_PROTOCOL".into()); }
        let mut entries = self.entries.clone();
        let existing = entries.iter().position(|e| e.profile.endpoint == endpoint);
        if existing.is_none() && entries.len() >= 16 { return Err("REMOTE_CONNECTION_LIMIT".into()); }
        let profile = if let Some(i) = existing {
            let mut p = entries[i].profile.clone(); p.name = name.trim().into(); p
        } else { Profile { id: uuid::Uuid::new_v4().to_string(), name: name.trim().into(), endpoint, adapter: "dsh".into(), session_id: String::new(), auto_reconnect: true, uncertain_sessions: vec![] } };
        let entry = Entry { profile: profile.clone(), cookie };
        if let Some(i) = existing { entries[i] = entry; } else { entries.push(entry); }
        self.commit(entries)?;
        Ok(profile)
    }
    pub fn update(&mut self, id: &str, session: Option<String>, automatic: Option<bool>, uncertain: Option<(String,bool)>) -> Result<(), String> {
        let mut entries = self.entries.clone();
        let entry = entries.iter_mut().find(|e| e.profile.id == id).ok_or("REMOTE_NOT_FOUND")?;
        if let Some(session) = session {
            if session.len() > 512 { return Err("REMOTE_PROTOCOL".into()); } entry.profile.session_id = session;
        }
        if let Some(value) = automatic { entry.profile.auto_reconnect = value; }
        if let Some((session, pending)) = uncertain {
            entry.profile.uncertain_sessions.retain(|s| s != &session);
            if pending { entry.profile.uncertain_sessions.push(session); }
        }
        self.commit(entries)
    }
    pub fn forget(&mut self, id: &str) -> Result<(), String> {
        let entries = self.entries.iter().filter(|e| e.profile.id != id).cloned().collect(); self.commit(entries)
    }
}

pub(super) static VAULT: once_cell::sync::Lazy<tokio::sync::Mutex<Option<Vault>>> = once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(None));
pub(super) fn unlocked(value: &mut Option<Vault>) -> Result<&mut Vault, String> { value.as_mut().ok_or_else(|| LOCKED.into()) }

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_protector(envelope: Option<&[u8]>) -> Result<super::super::system_protection::ProtectedKey, String> {
        if let Some(value) = envelope { assert_eq!(value, b"test-only-envelope"); }
        Ok(super::super::system_protection::ProtectedKey { key: [42;32], envelope: b"test-only-envelope".to_vec() })
    }
    #[test]
    fn system_vault_cancel_migrate_failure_and_reopen_preserve_data() {
        let dir = std::env::temp_dir().join(format!("bob-system-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); let path = dir.join("vault.db");
        let mut old = Vault::unlock(path.clone(), "legacy-password-123").unwrap();
        let p = old.save("DSH".into(), "https://test.invalid/".into(), "legacy-cookie-secret".into()).unwrap();
        old.update(&p.id, Some("session-old".into()), None, Some(("session-old".into(), true))).unwrap(); drop(old);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(Vault::unlock_system(path.clone(), None, |_| panic!("must not invoke OS before migration choice")).err().unwrap(), "REMOTE_VAULT_MIGRATION_REQUIRED");
        assert!(Vault::unlock_system(path.clone(), Some("wrong-password-123"), |_| panic!("must decrypt legacy first")).is_err());
        assert!(Vault::unlock_system(path.clone(), Some("legacy-password-123"), |_| Err("REMOTE_SYSTEM_AUTH_CANCELLED".into())).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        let db = database(&path).unwrap();
        db.execute_batch("CREATE TRIGGER deny_update BEFORE UPDATE ON vault BEGIN SELECT RAISE(ABORT, 'migration write failure'); END;").unwrap();
        assert!(Vault::unlock_system(path.clone(), Some("legacy-password-123"), fixture_protector).is_err());
        assert_eq!(Vault::unlock(path.clone(), "legacy-password-123").unwrap().credential(&p.id).unwrap().1, "legacy-cookie-secret");
        db.execute_batch("DROP TRIGGER deny_update;").unwrap(); drop(db);
        let migrated = Vault::unlock_system(path.clone(), Some("legacy-password-123"), fixture_protector).unwrap();
        assert_eq!(migrated.list()[0].id, p.id); drop(migrated);
        assert!(Vault::unlock(path.clone(), "legacy-password-123").is_err());
        let mut reopened = Vault::unlock_system(path.clone(), None, fixture_protector).unwrap();
        assert_eq!(reopened.list()[0].session_id, "session-old");
        assert_eq!(reopened.list()[0].uncertain_sessions, ["session-old"]);
        assert_eq!(reopened.credential(&p.id).unwrap().1, "legacy-cookie-secret");
        reopened.forget(&p.id).unwrap(); drop(reopened);
        assert!(Vault::unlock_system(path.clone(), None, fixture_protector).unwrap().list().is_empty());
        database(&path).unwrap().execute("UPDATE vault SET ciphertext=zeroblob(40)", []).unwrap();
        assert!(Vault::unlock_system(path, None, fixture_protector).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn system_auth_failure_does_not_create_vault_snapshot() {
        let dir = std::env::temp_dir().join(format!("bob-system-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); let path = dir.join("vault.db");
        assert!(Vault::unlock_system(path.clone(), None, |_| Err("REMOTE_SYSTEM_AUTH_CANCELLED".into())).is_err());
        assert_eq!(database(&path).unwrap().query_row("SELECT count(*) FROM vault", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        let mut vault = Vault::unlock_system(path.clone(), None, fixture_protector).unwrap();
        vault.save("Sample".into(), "https://host.invalid/".into(), "system-cookie-test-secret".into()).unwrap(); drop(vault);
        let before = std::fs::read(&path).unwrap();
        assert!(!before.windows(25).any(|v| v == b"system-cookie-test-secret"));
        assert!(Vault::unlock_system(path.clone(), None, |_| Err("REMOTE_SYSTEM_KEY_UNAVAILABLE".into())).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn system_vault_real_windows_persistence_without_bob_password() {
        let dir = std::env::temp_dir().join(format!("bob-dpapi-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); let path = dir.join("vault.db");
        let mut vault = Vault::unlock_system(path.clone(), None, super::super::system_protection::windows_unlock).unwrap();
        let profile = vault.save("Windows".into(), "https://test.invalid/".into(), "os-protected-cookie".into()).unwrap(); drop(vault);
        let vault = Vault::unlock_system(path, None, super::super::system_protection::windows_unlock).unwrap();
        assert_eq!(vault.credential(&profile.id).unwrap().1, "os-protected-cookie"); drop(vault);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn vault_failed_write_preserves_memory_and_durable_credential() {
        let dir = std::env::temp_dir().join(format!("bob-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); let path = dir.join("vault.db");
        let mut v = Vault::unlock(path.clone(), "test-password-123").unwrap();
        let p = v.save("Original".into(), "https://host.invalid/".into(), "dsh-auth-test=original".into()).unwrap();
        let conn = database(&path).unwrap();
        conn.execute_batch("CREATE TRIGGER deny_update BEFORE UPDATE ON vault BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
        assert!(v.forget(&p.id).is_err());
        assert!(v.save("Changed".into(), p.endpoint.clone(), "dsh-auth-test=replacement".into()).is_err());
        assert_eq!(v.credential(&p.id).unwrap().1, "dsh-auth-test=original");
        assert_eq!(v.list()[0].name, "Original");
        drop(v); drop(conn);
        let v = Vault::unlock(path, "test-password-123").unwrap();
        assert_eq!(v.credential(&p.id).unwrap().1, "dsh-auth-test=original");
        drop(v); std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn vault_roundtrip_wrong_password_tamper_forget_and_no_plaintext() {
        let dir = std::env::temp_dir().join(format!("bob-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); let path = dir.join("vault.db");
        let mut v = Vault::unlock(path.clone(), "test-password-123").unwrap();
        let p = v.save("DSH test".into(), "https://host.invalid/".into(), "dsh-auth-test=SECRET-COOKIE".into()).unwrap();
        v.update(&p.id, Some("s1".into()), None, Some(("s1".into(), true))).unwrap(); drop(v);
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(13).any(|w| w == b"SECRET-COOKIE"));
        assert!(Vault::unlock(path.clone(), "wrong-password-123").is_err());
        let mut v = Vault::unlock(path.clone(), "test-password-123").unwrap();
        assert_eq!(v.list()[0].session_id, "s1"); assert_eq!(v.list()[0].uncertain_sessions, vec!["s1"]);
        v.forget(&p.id).unwrap(); drop(v);
        assert!(Vault::unlock(path.clone(), "test-password-123").unwrap().list().is_empty());
        database(&path).unwrap().execute("UPDATE vault SET ciphertext=zeroblob(40)", []).unwrap();
        assert!(Vault::unlock(path.clone(), "test-password-123").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

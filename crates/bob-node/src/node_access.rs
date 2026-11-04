//! Node-local pairing authority. Secrets never leave this store as topology.
use ring::{
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{num::NonZeroU32, path::Path};
type Result<T> = std::result::Result<T, &'static str>;
const DAY: i64 = 86_400_000;
fn random() -> Result<String> {
    let mut b = [0u8; 32];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| "NODE_RANDOM_FAILED")?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}
fn digest(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn open(dir: &Path) -> Result<Connection> {
    let c = Connection::open(dir.join("node_access.sqlite")).map_err(|_| "NODE_ACCESS_STORE")?;
    c.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "NODE_ACCESS_STORE")?;
    c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS identity(id INTEGER PRIMARY KEY CHECK(id=1),node TEXT NOT NULL,salt BLOB NOT NULL,password BLOB NOT NULL); CREATE TABLE IF NOT EXISTS credentials(hash TEXT PRIMARY KEY,id TEXT NOT NULL,scope TEXT NOT NULL,label TEXT NOT NULL,expires INTEGER NOT NULL,revoked INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS invitations(id TEXT PRIMARY KEY,hash TEXT NOT NULL,expires INTEGER NOT NULL,used INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS attempts(kind TEXT PRIMARY KEY,count INTEGER NOT NULL,reset INTEGER NOT NULL);").map_err(|_|"NODE_ACCESS_STORE")?;
    Ok(c)
}
pub fn enabled(dir: &Path) -> bool {
    dir.join("node_access.sqlite").exists()
}
pub fn initialize(dir: &Path, password: &str) -> Result<()> {
    if password.len() < 12 || password.len() > 1024 {
        return Err("NODE_PASSWORD_LENGTH");
    }
    std::fs::create_dir_all(dir).map_err(|_| "NODE_ACCESS_STORE")?;
    let c = open(dir)?;
    let salt = random()?;
    let mut hash = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(600_000).unwrap(),
        salt.as_bytes(),
        password.as_bytes(),
        &mut hash,
    );
    c.execute(
        "INSERT INTO identity VALUES(1,?1,?2,?3)",
        params![random()?, salt.as_bytes(), hash.as_slice()],
    )
    .map_err(|_| "NODE_ALREADY_CONFIGURED")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join("node_access.sqlite"),
            std::fs::Permissions::from_mode(0o600),
        )
        .map_err(|_| "NODE_ACCESS_STORE")?;
    }
    Ok(())
}
pub fn identity(dir: &Path) -> Result<String> {
    open(dir)?
        .query_row("SELECT node FROM identity WHERE id=1", [], |r| r.get(0))
        .map_err(|_| "NODE_NOT_CONFIGURED")
}
fn rate(c: &Connection, kind: &str, now: i64) -> Result<()> {
    c.execute("INSERT INTO attempts VALUES(?1,1,?2) ON CONFLICT(kind) DO UPDATE SET count=CASE WHEN reset<=?3 THEN 1 ELSE count+1 END,reset=CASE WHEN reset<=?3 THEN ?2 ELSE reset END",params![kind,now+300_000,now]).map_err(|_|"NODE_ACCESS_STORE")?;
    let n: i64 = c
        .query_row("SELECT count FROM attempts WHERE kind=?1", [kind], |r| {
            r.get(0)
        })
        .map_err(|_| "NODE_ACCESS_STORE")?;
    if n > 8 {
        Err("NODE_RATE_LIMITED")
    } else {
        Ok(())
    }
}
fn token(c: &Connection, scope: &str, label: &str, now: i64) -> Result<Value> {
    let secret = random()?;
    let id = random()?;
    let expires = now
        + if scope == "owner" {
            3_600_000
        } else {
            365 * DAY
        };
    c.execute(
        "INSERT INTO credentials VALUES(?1,?2,?3,?4,?5,0)",
        params![digest(&secret), id, scope, label, expires],
    )
    .map_err(|_| "NODE_ACCESS_STORE")?;
    Ok(
        json!({"access_token":secret,"credential_id":id,"expires_at":expires,"scope":scope,"topology_sync":false}),
    )
}
pub fn authorize(dir: &Path, header: Option<&str>, now: i64) -> Result<String> {
    let raw = header
        .and_then(|h| h.strip_prefix("Bearer "))
        .filter(|s| s.len() == 64)
        .ok_or("NODE_AUTH_REQUIRED")?;
    open(dir)?
        .query_row(
            "SELECT scope FROM credentials WHERE hash=?1 AND revoked=0 AND expires>?2",
            params![digest(raw), now],
            |r| r.get(0),
        )
        .optional()
        .map_err(|_| "NODE_ACCESS_STORE")?
        .ok_or("NODE_AUTH_REQUIRED")
}
pub fn route(
    dir: &Path,
    method: &str,
    path: &str,
    header: Option<&str>,
    body: &Value,
    now: i64,
) -> Result<Value> {
    if method == "GET" && path == "info" {
        return Ok(
            json!({"protocol":"bob-node-v1","node_id":identity(dir)?,"authentication":"owner_password","pairing":"single_use_qr","credential_days":365}),
        );
    }
    if method != "POST" {
        return Err("NODE_METHOD_NOT_ALLOWED");
    }
    if path == "login" {
        let c = open(dir)?;
        rate(&c, "login", now)?;
        let password = body["password"]
            .as_str()
            .filter(|s| s.len() <= 1024)
            .ok_or("NODE_AUTH_REQUIRED")?;
        let (salt, hash): (Vec<u8>, Vec<u8>) = c
            .query_row("SELECT salt,password FROM identity WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(|_| "NODE_NOT_CONFIGURED")?;
        pbkdf2::verify(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(600_000).unwrap(),
            &salt,
            password.as_bytes(),
            &hash,
        )
        .map_err(|_| "NODE_AUTH_REQUIRED")?;
        return token(&c, "owner", "browser", now);
    }
    if path == "exchange" {
        let mut c = open(dir)?;
        rate(&c, "exchange", now)?;
        let id = body["pairing_id"]
            .as_str()
            .ok_or("NODE_INVITATION_INVALID")?;
        let code = body["code"]
            .as_str()
            .filter(|s| s.len() == 64)
            .ok_or("NODE_INVITATION_INVALID")?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "NODE_ACCESS_STORE")?;
        let changed = tx
            .execute(
                "UPDATE invitations SET used=1 WHERE id=?1 AND hash=?2 AND expires>?3 AND used=0",
                params![id, digest(code), now],
            )
            .map_err(|_| "NODE_ACCESS_STORE")?;
        if changed != 1 {
            return Err("NODE_INVITATION_INVALID");
        }
        let label = body["device_name"].as_str().unwrap_or("Bob device");
        if label.len() > 160 {
            return Err("NODE_LABEL_INVALID");
        }
        let mut result = token(&tx, "device", label, now)?;
        result["node_id"] = json!(identity(dir)?);
        tx.commit().map_err(|_| "NODE_ACCESS_STORE")?;
        return Ok(result);
    }
    let scope = authorize(dir, header, now)?;
    if path == "revoke" {
        if let Some(id) = body.get("credential_id").and_then(Value::as_str) {
            if scope != "owner" {
                return Err("NODE_OWNER_REQUIRED");
            }
            open(dir)?
                .execute("UPDATE credentials SET revoked=1 WHERE id=?1", [id])
                .map_err(|_| "NODE_ACCESS_STORE")?;
        } else {
            let raw = header.unwrap().strip_prefix("Bearer ").unwrap();
            open(dir)?
                .execute(
                    "UPDATE credentials SET revoked=1 WHERE hash=?1",
                    [digest(raw)],
                )
                .map_err(|_| "NODE_ACCESS_STORE")?;
        }
        return Ok(json!({"revoked":true}));
    }
    if scope != "owner" {
        return Err("NODE_OWNER_REQUIRED");
    }
    if path == "credentials" {
        let c = open(dir)?;
        let mut query=c.prepare("SELECT id,label,expires FROM credentials WHERE scope='device' AND revoked=0 AND expires>?1 ORDER BY expires DESC").map_err(|_|"NODE_ACCESS_STORE")?;
        let rows=query.query_map([now],|r|Ok(json!({"id":r.get::<_,String>(0)?,"label":r.get::<_,String>(1)?,"expires_at":r.get::<_,i64>(2)?}))).map_err(|_|"NODE_ACCESS_STORE")?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|_| "NODE_ACCESS_STORE")?)
        }
        return Ok(json!({"credentials":items}));
    }
    if path == "invitations" {
        let c = open(dir)?;
        let id = random()?;
        let secret = random()?;
        let expires = now + 300_000;
        c.execute("DELETE FROM invitations WHERE expires<=?1 OR used=1", [now])
            .map_err(|_| "NODE_ACCESS_STORE")?;
        c.execute(
            "INSERT INTO invitations VALUES(?1,?2,?3,0)",
            params![id, digest(&secret), expires],
        )
        .map_err(|_| "NODE_ACCESS_STORE")?;
        return Ok(
            json!({"pairing_id":id,"user_code":secret,"expires_at":expires,"node_id":identity(dir)?}),
        );
    }
    Err("NODE_ROUTE_NOT_FOUND")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_invitation_restart_replay_and_revocation() {
        let d = tempfile::tempdir().unwrap();
        initialize(d.path(), "test-password-long").unwrap();
        assert!(route(
            d.path(),
            "POST",
            "login",
            None,
            &json!({"password":"wrong"}),
            1
        )
        .is_err());
        let owner = route(
            d.path(),
            "POST",
            "login",
            None,
            &json!({"password":"test-password-long"}),
            2,
        )
        .unwrap();
        let h = format!("Bearer {}", owner["access_token"].as_str().unwrap());
        assert!(route(d.path(), "POST", "invitations", None, &json!({}), 3).is_err());
        let inv = route(d.path(), "POST", "invitations", Some(&h), &json!({}), 4).unwrap();
        let mut body = json!({"pairing_id":inv["pairing_id"],"code":"0".repeat(64)});
        assert!(route(d.path(), "POST", "exchange", None, &body, 5).is_err());
        body["code"] = inv["user_code"].clone();
        let device = route(d.path(), "POST", "exchange", None, &body, 6).unwrap();
        assert!(route(d.path(), "POST", "exchange", None, &body, 7).is_err());
        let dh = format!("Bearer {}", device["access_token"].as_str().unwrap());
        assert_eq!(authorize(d.path(), Some(&dh), 8).unwrap(), "device");
        assert!(route(d.path(), "POST", "invitations", Some(&dh), &json!({}), 8).is_err());
        route(d.path(), "POST", "revoke", Some(&dh), &json!({}), 9).unwrap();
        assert!(authorize(d.path(), Some(&dh), 10).is_err());
        assert!(authorize(d.path(), Some(&h), 3_600_003).is_err());
    }
    #[test]
    fn expiration_and_rate_limit() {
        let d = tempfile::tempdir().unwrap();
        initialize(d.path(), "test-password-long").unwrap();
        let c = open(d.path()).unwrap();
        c.execute(
            "INSERT INTO invitations VALUES('expired',?1,1,0)",
            [digest(&"a".repeat(64))],
        )
        .unwrap();
        let body = json!({"pairing_id":"expired","code":"a".repeat(64)});
        for _ in 0..8 {
            assert!(route(d.path(), "POST", "exchange", None, &body, 10).is_err())
        }
        assert_eq!(
            route(d.path(), "POST", "exchange", None, &body, 10).unwrap_err(),
            "NODE_RATE_LIMITED"
        );
    }
}

//! Immutable first-pair connection metadata. No IPC accepts a renderer's claim
//! that pairing completed. Only authenticated protocol confirmation plus the
//! native local confirmation flow may persist a new connection.
use crate::{
    device_identity::{NativeIdentity, PublicIdentity},
    home_link::PairedHome,
};
use bob_transport::{PinnedClient, ServerName, TlsIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
};

const CONFIG: &str = "paired-home.json";
const PENDING: &str = "paired-home.pending";
const SEAL: &str = "paired-home.sha256";
const LIMIT: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HomeConfig {
    pub version: u8,
    pub invitation_id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub home_certificate_der: Vec<u8>,
    pub home_certificate_sha256: String,
    pub tls_server_name: String,
    pub address: SocketAddr,
    pub local_device_id: String,
    pub local_certificate_sha256: String,
    pub session_id: String,
    pub authorization_revision: i64,
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn pin(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl HomeConfig {
    fn validate(&self, local: &PublicIdentity, session: &str) -> Result<(), &'static str> {
        if self.version != 1
            || self.home_epoch < 1
            || self.authorization_revision < 1
            || !valid_id(&self.invitation_id)
            || !valid_id(&self.home_device_id)
            || !valid_id(&self.local_device_id)
            || !valid_id(&self.session_id)
            || self.home_certificate_der.is_empty()
            || self.home_certificate_der.len() > 16384
            || !pin(&self.home_certificate_sha256)
            || !pin(&self.local_certificate_sha256)
            || self.local_device_id != local.device_id
            || self.local_certificate_sha256 != local.certificate_sha256
            || self.session_id != session
            || self.home_device_id == self.local_device_id
            || self.address.port() == 0
            || self.address.ip().is_unspecified()
            || self.address.ip().is_multicast()
            || self.address.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
            || ServerName::try_from(self.tls_server_name.clone()).is_err()
            || self.home_certificate_sha256
                != format!("{:x}", Sha256::digest(&self.home_certificate_der))
        {
            return Err("HOME_CONFIG_INVALID");
        }
        Ok(())
    }
    pub(crate) fn connect(&self, local: &NativeIdentity) -> Result<PairedHome, &'static str> {
        // The caller must load/validate against the actual kernel session first.
        self.validate(&local.public, &self.session_id)?;
        let client = PinnedClient::new(
            TlsIdentity {
                certificates: vec![local.public.certificate_der.clone().into()],
                private_key: local.private_key.clone_key(),
            },
            vec![self.home_certificate_der.clone().into()],
            self.home_certificate_sha256.clone(),
        )
        .map_err(|_| "HOME_CONFIG_INVALID")?;
        Ok(PairedHome {
            client,
            address: self.address,
            server_name: ServerName::try_from(self.tls_server_name.clone())
                .map_err(|_| "HOME_CONFIG_INVALID")?,
        })
    }
}

fn read_bounded(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, &'static str> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("HOME_CONFIG_UNAVAILABLE"),
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => return Err("HOME_CONFIG_INVALID"),
        _ => {}
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "HOME_CONFIG_UNAVAILABLE")?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "HOME_CONFIG_UNAVAILABLE")?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err("HOME_CONFIG_INVALID");
    }
    Ok(Some(bytes))
}
fn create_file(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "HOME_CONFIG_WRITE_UNCONFIRMED")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "HOME_CONFIG_WRITE_UNCONFIRMED")
}
fn persist(root: &Path, config: &HomeConfig) -> Result<(), &'static str> {
    for name in [CONFIG, PENDING, SEAL] {
        match std::fs::symlink_metadata(root.join(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("HOME_CONFIG_ALREADY_PRESENT"),
        }
    }
    let bytes = serde_json::to_vec(config).map_err(|_| "HOME_CONFIG_INVALID")?;
    if bytes.len() > LIMIT {
        return Err("HOME_CONFIG_INVALID");
    }
    create_file(&root.join(PENDING), &bytes)?;
    create_file(
        &root.join(SEAL),
        format!("{:x}", Sha256::digest(&bytes)).as_bytes(),
    )?;
    std::fs::rename(root.join(PENDING), root.join(CONFIG))
        .map_err(|_| "HOME_CONFIG_WRITE_UNCONFIRMED")
}

/// Caller is the native confirmation action, after the person accepts the exact
/// displayed peer. The proof cannot be constructed from renderer JSON.
pub(crate) fn persist_after_local_confirmation(
    root: &Path,
    config: &HomeConfig,
    local: &PublicIdentity,
    session: &str,
    proof: &bob_transport::pairing::AuthenticatedPairingConfirmation,
) -> Result<(), &'static str> {
    config.validate(local, session)?;
    if proof.invitation_id() != config.invitation_id
        || proof.home_id() != config.home_device_id
        || proof.home_epoch() != config.home_epoch
        || proof.home_pin() != config.home_certificate_sha256
        || proof.candidate_id() != config.local_device_id
        || proof.candidate_pin() != config.local_certificate_sha256
        || proof.session_id() != config.session_id
        || proof.authorization_revision() != config.authorization_revision
    {
        return Err("HOME_PAIRING_CONFIRMATION_MISMATCH");
    }
    if let Some(old) = load(root, local, session)? {
        let old_bytes = serde_json::to_vec(&old).map_err(|_| "HOME_CONFIG_INVALID")?;
        let new_bytes = serde_json::to_vec(config).map_err(|_| "HOME_CONFIG_INVALID")?;
        if old_bytes == new_bytes {
            return Ok(());
        }
        if old.home_device_id != config.home_device_id
            || old.home_epoch != config.home_epoch
            || old.home_certificate_sha256 != config.home_certificate_sha256
            || old.local_device_id != config.local_device_id
            || old.local_certificate_sha256 != config.local_certificate_sha256
            || old.session_id != config.session_id
            || old.authorization_revision >= config.authorization_revision
        {
            return Err("HOME_CONFIG_REPLACEMENT_DENIED");
        }
        // A newly authenticated, higher-revision scan replaces only the same
        // Home/session. Old metadata is retained; torn writes remain fail-closed.
        let archive = root.join(format!(
            "paired-home-history-{:x}",
            Sha256::digest(old_bytes)
        ));
        std::fs::create_dir(&archive).map_err(|_| "HOME_CONFIG_WRITE_UNCONFIRMED")?;
        for name in [CONFIG, SEAL] {
            std::fs::rename(root.join(name), archive.join(name))
                .map_err(|_| "HOME_CONFIG_WRITE_UNCONFIRMED")?;
        }
    }
    persist(root, config)
}

pub(crate) fn load(
    root: &Path,
    local: &PublicIdentity,
    session: &str,
) -> Result<Option<HomeConfig>, &'static str> {
    match std::fs::symlink_metadata(root.join(PENDING)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err("HOME_CONFIG_WRITE_UNCONFIRMED"),
    }
    let body = read_bounded(&root.join(CONFIG), LIMIT)?;
    let seal = read_bounded(&root.join(SEAL), 64)?;
    let (body, seal) = match (body, seal) {
        (None, None) => return Ok(None),
        (Some(body), Some(seal)) => (body, seal),
        _ => return Err("HOME_CONFIG_WRITE_UNCONFIRMED"),
    };
    if seal != format!("{:x}", Sha256::digest(&body)).as_bytes() {
        return Err("HOME_CONFIG_CHANGED");
    }
    let config: HomeConfig = serde_json::from_slice(&body).map_err(|_| "HOME_CONFIG_INVALID")?;
    config.validate(local, session)?;
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PublicIdentity, HomeConfig) {
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["home.invalid".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap()
            .der()
            .to_vec();
        let local = PublicIdentity {
            device_id: "phone-fixture".into(),
            certificate_sha256: "a".repeat(64),
            certificate_der: vec![],
            tls_server_name: "local.invalid".into(),
        };
        let config = HomeConfig {
            version: 1,
            invitation_id: "test-invite".into(),
            home_device_id: "home-fixture".into(),
            home_epoch: 1,
            home_certificate_sha256: format!("{:x}", Sha256::digest(&cert)),
            home_certificate_der: cert,
            tls_server_name: "home.invalid".into(),
            address: "127.0.0.1:32123".parse().unwrap(),
            local_device_id: local.device_id.clone(),
            local_certificate_sha256: local.certificate_sha256.clone(),
            session_id: "test-session".into(),
            authorization_revision: 1,
        };
        (local, config)
    }
    #[test]
    fn corruption_replacement_and_incomplete_write_fail_closed() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("bob-home-config-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let (local, config) = fixture();
        assert!(load(&root, &local, "test-session").unwrap().is_none());
        // Storage unit test only; this private helper is not pairing evidence.
        persist(&root, &config).unwrap();
        assert_eq!(
            load(&root, &local, "test-session")
                .unwrap()
                .unwrap()
                .home_device_id,
            "home-fixture"
        );
        assert!(load(&root, &local, "other-session").is_err());
        assert!(persist(&root, &config).is_err());
        std::fs::write(root.join(CONFIG), b"{}").unwrap();
        assert!(load(&root, &local, "test-session").is_err());
        std::fs::remove_file(root.join(CONFIG)).unwrap();
        assert!(load(&root, &local, "test-session").is_err());
        std::fs::remove_file(root.join(SEAL)).unwrap();
        std::fs::write(root.join(PENDING), b"pending").unwrap();
        assert!(load(&root, &local, "test-session").is_err());
        std::fs::remove_file(root.join(PENDING)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn identity_pin_and_destination_validation() {
        let (local, mut config) = fixture();
        config.validate(&local, "test-session").unwrap();
        config.home_certificate_der.push(0);
        assert!(config.validate(&local, "test-session").is_err());
        let (_, mut config) = fixture();
        config.local_certificate_sha256 = "b".repeat(64);
        assert!(config.validate(&local, "test-session").is_err());
        let (_, mut config) = fixture();
        config.address = "0.0.0.0:0".parse().unwrap();
        assert!(config.validate(&local, "test-session").is_err());
    }
}

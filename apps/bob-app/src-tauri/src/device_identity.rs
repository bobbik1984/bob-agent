//! Locally generated transport identity. The renderer receives only PublicIdentity.
//! Pairing permission is separate: possessing this certificate grants no Home access.
use rcgen::{CertificateParams, ExtendedKeyUsagePurpose, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, Runtime};

const RECORD_LIMIT: usize = 64 * 1024;
const CERT_LIMIT: usize = 16 * 1024;
const KEY_LIMIT: usize = 8 * 1024;
// Reserved protocol DNS identity; endpoint addresses are configured separately.
const TLS_NAME: &str = "bob-device.invalid";
const MARKER: &str = "device-identity.fingerprint";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicIdentity {
    pub device_id: String,
    pub certificate_sha256: String,
    pub certificate_der: Vec<u8>,
    pub tls_server_name: String,
}

pub(crate) struct NativeIdentity {
    pub public: PublicIdentity,
    pub private_key: PrivateKeyDer<'static>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateRecord {
    version: u8,
    device_id: String,
    certificate: Vec<u8>,
    key_pkcs8: Vec<u8>,
}
impl Drop for PrivateRecord {
    fn drop(&mut self) {
        for byte in &mut self.key_pkcs8 {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

fn generate(device_id: String) -> Result<Vec<u8>, &'static str> {
    if device_id.is_empty() || device_id.len() > 128 || device_id.chars().any(char::is_control) {
        return Err("IDENTITY_INVALID_DEVICE");
    }
    let key = KeyPair::generate().map_err(|_| "IDENTITY_GENERATION_FAILED")?;
    let mut params = CertificateParams::new(vec![TLS_NAME.to_owned()])
        .map_err(|_| "IDENTITY_GENERATION_FAILED")?;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::minutes(5);
    params.not_after = now + time::Duration::days(365);
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    let cert = params
        .self_signed(&key)
        .map_err(|_| "IDENTITY_GENERATION_FAILED")?;
    serde_json::to_vec(&PrivateRecord {
        version: 1,
        device_id,
        certificate: cert.der().to_vec(),
        key_pkcs8: key.serialize_der(),
    })
    .map_err(|_| "IDENTITY_GENERATION_FAILED")
}

fn decode(bytes: &[u8]) -> Result<NativeIdentity, &'static str> {
    if bytes.is_empty() || bytes.len() > RECORD_LIMIT {
        return Err("IDENTITY_INVALID");
    }
    let record: PrivateRecord = serde_json::from_slice(bytes).map_err(|_| "IDENTITY_INVALID")?;
    if record.version != 1
        || record.device_id.is_empty()
        || record.device_id.len() > 128
        || record.device_id.chars().any(char::is_control)
        || record.certificate.is_empty()
        || record.certificate.len() > CERT_LIMIT
        || record.key_pkcs8.is_empty()
        || record.key_pkcs8.len() > KEY_LIMIT
    {
        return Err("IDENTITY_INVALID");
    }
    let private_key: PrivateKeyDer<'static> =
        PrivatePkcs8KeyDer::from(record.key_pkcs8.clone()).into();
    let cert = CertificateDer::from(record.certificate.clone());
    let checked = rustls::sign::CertifiedKey::from_der(
        vec![cert],
        private_key.clone_key(),
        &rustls::crypto::ring::default_provider(),
    )
    .map_err(|_| "IDENTITY_INVALID")?;
    // from_der accepts an unknown consistency result; our generated ECDSA key
    // supports explicit public-key matching, so unknown must fail here.
    checked.keys_match().map_err(|_| "IDENTITY_INVALID")?;
    let fingerprint = format!("{:x}", Sha256::digest(&record.certificate));
    Ok(NativeIdentity {
        public: PublicIdentity {
            device_id: record.device_id.clone(),
            certificate_sha256: fingerprint,
            certificate_der: record.certificate.clone(),
            tls_server_name: TLS_NAME.to_owned(),
        },
        private_key,
    })
}

fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, &'static str> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| "IDENTITY_STORAGE_UNAVAILABLE")?;
    std::fs::create_dir_all(&root).map_err(|_| "IDENTITY_STORAGE_UNAVAILABLE")?;
    Ok(root)
}

fn marker(root: &Path) -> Result<Option<String>, &'static str> {
    let path = root.join(MARKER);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("IDENTITY_STORAGE_UNAVAILABLE"),
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
            return Err("IDENTITY_MARKER_INVALID")
        }
        _ => {}
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "IDENTITY_STORAGE_UNAVAILABLE")?
        .take(65)
        .read_to_end(&mut bytes)
        .map_err(|_| "IDENTITY_STORAGE_UNAVAILABLE")?;
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err("IDENTITY_MARKER_INVALID");
    }
    Ok(Some(
        String::from_utf8(bytes).map_err(|_| "IDENTITY_MARKER_INVALID")?,
    ))
}

fn mark(root: &Path, fingerprint: &str) -> Result<(), &'static str> {
    if let Some(existing) = marker(root)? {
        return if existing == fingerprint {
            Ok(())
        } else {
            Err("IDENTITY_CHANGED")
        };
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(MARKER))
        .map_err(|_| "IDENTITY_STORAGE_UNAVAILABLE")?;
    file.write_all(fingerprint.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "IDENTITY_STORAGE_UNAVAILABLE")
}

async fn vault_read<R: Runtime>(
    app: &AppHandle<R>,
    root: PathBuf,
) -> Result<Option<Vec<u8>>, &'static str> {
    #[cfg(target_os = "android")]
    {
        let _ = root;
        crate::android_vault::read(app).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        tauri::async_runtime::spawn_blocking(move || match crate::device_vault::read(&root) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(crate::device_vault::VaultError::NotFound) => Ok(None),
            Err(_) => Err("IDENTITY_VAULT_UNAVAILABLE"),
        })
        .await
        .map_err(|_| "IDENTITY_VAULT_UNAVAILABLE")?
    }
}

async fn vault_write<R: Runtime>(
    app: &AppHandle<R>,
    root: PathBuf,
    mut bytes: Vec<u8>,
) -> Result<(), &'static str> {
    #[cfg(target_os = "android")]
    {
        let _ = root;
        crate::android_vault::store_new(app, bytes).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        tauri::async_runtime::spawn_blocking(move || {
            let result = crate::device_vault::store_new(&root, &bytes)
                .map_err(|_| "IDENTITY_VAULT_UNAVAILABLE");
            bytes.fill(0);
            result
        })
        .await
        .map_err(|_| "IDENTITY_VAULT_UNAVAILABLE")?
    }
}

/// Native transport only. Never expose this return type through a Tauri command.
pub(crate) async fn load_native<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<NativeIdentity>, &'static str> {
    let root = root(app)?;
    let expected = marker(&root)?;
    let Some(mut bytes) = vault_read(app, root.clone()).await? else {
        return if expected.is_some() {
            Err("IDENTITY_MISSING_REPAIR_REQUIRED")
        } else {
            Ok(None)
        };
    };
    let result = decode(&bytes);
    bytes.fill(0);
    let identity = result?;
    if expected
        .as_ref()
        .is_some_and(|p| p != &identity.public.certificate_sha256)
    {
        return Err("IDENTITY_CHANGED");
    }
    // Recovers only the public marker after vault commit; never changes a key.
    mark(&root, &identity.public.certificate_sha256)?;
    Ok(Some(identity))
}

pub(crate) async fn public<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<PublicIdentity>, &'static str> {
    Ok(load_native(app).await?.map(|i| i.public))
}

/// Invoke only from an explicit native pairing action, not application startup.
pub(crate) async fn create<R: Runtime>(
    app: &AppHandle<R>,
    local_device_id: String,
) -> Result<PublicIdentity, &'static str> {
    if let Some(identity) = load_native(app).await? {
        return if identity.public.device_id == local_device_id {
            Ok(identity.public)
        } else {
            Err("IDENTITY_DEVICE_MISMATCH")
        };
    }
    let root = root(app)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || generate(local_device_id))
        .await
        .map_err(|_| "IDENTITY_GENERATION_FAILED")??;
    vault_write(app, root, bytes).await?;
    public(app).await?.ok_or("IDENTITY_SAVE_UNCONFIRMED")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_export_has_no_private_key_and_identity_survives_decode() {
        let bytes = generate("local-device-test".to_owned()).unwrap();
        let identity = decode(&bytes).unwrap();
        let public = serde_json::to_value(&identity.public).unwrap();
        assert_eq!(public.as_object().unwrap().len(), 4);
        assert!(public.get("key_pkcs8").is_none());
        assert_eq!(
            identity.public.certificate_sha256,
            decode(&bytes).unwrap().public.certificate_sha256
        );
        assert_eq!(identity.public.device_id, "local-device-test");
    }
    #[test]
    fn malformed_oversized_and_mismatched_key_are_rejected() {
        assert!(decode(b"{}").is_err());
        assert!(decode(&vec![0; RECORD_LIMIT + 1]).is_err());
        let mut first: PrivateRecord =
            serde_json::from_slice(&generate("one".to_owned()).unwrap()).unwrap();
        let second: PrivateRecord =
            serde_json::from_slice(&generate("two".to_owned()).unwrap()).unwrap();
        first.key_pkcs8 = second.key_pkcs8.clone();
        assert!(decode(&serde_json::to_vec(&first).unwrap()).is_err());
        assert!(generate(String::new()).is_err());
        first.version = 2;
        assert!(decode(&serde_json::to_vec(&first).unwrap()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn real_protected_storage_roundtrip_and_marker_conflict() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "bob-identity-canary-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut bytes = generate("canary-device".into()).unwrap();
        let original = decode(&bytes).unwrap();
        crate::device_vault::store_new(&root, &bytes).unwrap();
        assert!(crate::device_vault::store_new(&root, &bytes).is_err());
        bytes.fill(0);
        let mut restored = crate::device_vault::read(&root).unwrap();
        let loaded = decode(&restored).unwrap();
        restored.fill(0);
        assert_eq!(
            loaded.public.certificate_sha256,
            original.public.certificate_sha256
        );
        mark(&root, &loaded.public.certificate_sha256).unwrap();
        assert_eq!(
            marker(&root).unwrap(),
            Some(loaded.public.certificate_sha256.clone())
        );
        assert_eq!(mark(&root, &"0".repeat(64)), Err("IDENTITY_CHANGED"));
        std::fs::remove_file(root.join("device-identity.dpapi")).unwrap();
        assert_eq!(
            crate::device_vault::read(&root),
            Err(crate::device_vault::VaultError::NotFound)
        );
        // The established marker remains after losing the protected record.
        assert!(marker(&root).unwrap().is_some());
        std::fs::remove_file(root.join(MARKER)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

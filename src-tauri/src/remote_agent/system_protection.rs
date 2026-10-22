//! Native-only key protection. No device password, PIN or biometric data crosses the bridge.
pub struct ProtectedKey { pub key: [u8; 32], pub envelope: Vec<u8> }
impl Drop for ProtectedKey { fn drop(&mut self) { self.key.fill(0); } }

pub fn unlock(_app: &tauri::AppHandle, envelope: Option<&[u8]>) -> Result<ProtectedKey, String> {
    #[cfg(windows)]
    { windows_unlock(envelope) }
    #[cfg(target_os = "android")]
    {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use tauri::Manager;
        let response = _app.state::<tauri_plugin_device_vault::DeviceVault<tauri::Wry>>()
            .unlock(envelope.map(|v| STANDARD.encode(v)))?;
        if let Some(error) = response.error { return Err(error); }
        let mut bytes = STANDARD.decode(response.key).map_err(|_| "REMOTE_SYSTEM_KEY_UNAVAILABLE")?;
        let key: [u8;32] = bytes.as_slice().try_into().map_err(|_| "REMOTE_SYSTEM_KEY_UNAVAILABLE")?;
        bytes.fill(0);
        let envelope = STANDARD.decode(response.envelope).map_err(|_| "REMOTE_SYSTEM_KEY_UNAVAILABLE")?;
        if envelope.len() != 60 { return Err("REMOTE_SYSTEM_KEY_UNAVAILABLE".into()); }
        Ok(ProtectedKey { key, envelope })
    }
    #[cfg(not(any(windows, target_os = "android")))]
    { let _ = envelope; Err("REMOTE_SYSTEM_UNSUPPORTED".into()) }
}

#[cfg(windows)]
fn dpapi(input: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN}};
    if input.is_empty() || input.len() > 16384 { return Err("REMOTE_SYSTEM_KEY_UNAVAILABLE".into()); }
    let blob = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // CurrentUser scope (never LOCAL_MACHINE). Windows manages the wrapping key.
    let ok = unsafe {
        if protect { CryptProtectData(&blob, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
        else { CryptUnprotectData(&blob, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
    };
    if ok == 0 { return Err("REMOTE_SYSTEM_KEY_UNAVAILABLE".into()); }
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe {
        // Clear the OS-owned plaintext buffer before releasing it.
        for i in 0..out.cbData as usize { std::ptr::write_volatile(out.pbData.add(i), 0); }
        LocalFree(out.pbData as _);
    }
    Ok(bytes)
}

#[cfg(windows)]
pub(super) fn windows_unlock(envelope: Option<&[u8]>) -> Result<ProtectedKey, String> {
    use rand::RngCore;
    match envelope {
        Some(envelope) => {
            let mut plain = dpapi(envelope, false)?;
            let key = plain.as_slice().try_into().map_err(|_| "REMOTE_SYSTEM_KEY_UNAVAILABLE")?;
            plain.fill(0);
            Ok(ProtectedKey { key, envelope: envelope.to_vec() })
        }
        None => {
            let mut key = [0;32]; rand::rngs::OsRng.fill_bytes(&mut key);
            match dpapi(&key, true) {
                Ok(envelope) => Ok(ProtectedKey { key, envelope }),
                Err(e) => { key.fill(0); Err(e) }
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn dpapi_current_user_roundtrip_and_tamper_rejection() {
        let original = windows_unlock(None).unwrap();
        let reopened = windows_unlock(Some(&original.envelope)).unwrap();
        assert_eq!(original.key, reopened.key);
        assert!(!original.envelope.windows(32).any(|v| v == original.key));
        let mut corrupted = original.envelope.clone(); let n = corrupted.len(); corrupted[n-1] ^= 1;
        assert!(windows_unlock(Some(&corrupted)).is_err());
        assert!(windows_unlock(Some(&[])).is_err());
    }
}

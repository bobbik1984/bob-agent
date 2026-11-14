use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg_attr(not(target_os = "android"), allow(unused_imports))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg_attr(not(target_os = "android"), allow(unused_imports))]
use tauri::{AppHandle, Manager, Runtime, State};

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Default)]
pub(crate) struct SecurityLockState(pub(crate) AtomicBool);

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SecurityLockConfig {
    pub version: u8,
    pub decided: bool,
    pub enabled: bool,
}

impl Default for SecurityLockConfig {
    fn default() -> Self {
        Self {
            version: 1,
            decided: false,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SecurityLockStatus {
    pub supported: bool,
    pub enrolled: bool,
    pub decided: bool,
    pub enabled: bool,
    pub unlocked: bool,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn config_path(root: &Path) -> std::path::PathBuf {
    root.join("security_lock.json")
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn load_config(root: &Path) -> SecurityLockConfig {
    let path = config_path(root);
    let Ok(bytes) = std::fs::read(&path) else {
        return SecurityLockConfig::default();
    };
    serde_json::from_slice::<SecurityLockConfig>(&bytes).unwrap_or_default()
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn save_config(root: &Path, config: &SecurityLockConfig) -> Result<(), &'static str> {
    std::fs::create_dir_all(root).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let bytes = serde_json::to_vec(config).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let target = config_path(root);
    let pending = root.join("security_lock.json.pending");
    std::fs::write(&pending, &bytes).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    if std::fs::rename(&pending, &target).is_err() {
        let _ = std::fs::remove_file(&target);
        std::fs::rename(&pending, &target).map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    }
    Ok(())
}

#[cfg(target_os = "android")]
async fn query_capability<R: Runtime>(app: &AppHandle<R>) -> (bool, bool) {
    match crate::android_vault::check_device_auth(app).await {
        Ok(cap) => (cap.supported, cap.enrolled),
        Err(_) => (true, false),
    }
}

#[cfg(target_os = "android")]
fn build_status(
    supported: bool,
    enrolled: bool,
    cfg: &SecurityLockConfig,
    unlocked_flag: &AtomicBool,
) -> SecurityLockStatus {
    let unlocked = !cfg.enabled || unlocked_flag.load(Ordering::SeqCst);
    SecurityLockStatus {
        supported,
        enrolled,
        decided: cfg.decided,
        enabled: cfg.enabled,
        unlocked,
    }
}

#[cfg(not(target_os = "android"))]
fn non_mobile_status() -> SecurityLockStatus {
    SecurityLockStatus {
        supported: false,
        enrolled: false,
        decided: true,
        enabled: false,
        unlocked: true,
    }
}

#[cfg(target_os = "android")]
async fn verify_owner<R: Runtime>(
    app: &AppHandle<R>,
    title: Option<String>,
    subtitle: Option<String>,
) -> Result<(), &'static str> {
    let clean_title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Bob")
        .to_owned();
    let clean_subtitle = subtitle.unwrap_or_default().trim().to_owned();
    let outcome = crate::android_vault::authenticate_device_owner(app, clean_title, clean_subtitle).await?;
    if outcome.authenticated {
        return Ok(());
    }
    match outcome.status.as_str() {
        "not_enrolled" => Err("SECURITY_LOCK_NOT_ENROLLED"),
        "cancelled" => Err("SECURITY_LOCK_CANCELLED"),
        _ => Err("SECURITY_LOCK_AUTH_FAILED"),
    }
}

#[tauri::command]
pub(crate) async fn app_security_lock_status<R: Runtime>(
    app: AppHandle<R>,
    lock_state: State<'_, SecurityLockState>,
) -> Result<SecurityLockStatus, &'static str> {
    #[cfg(target_os = "android")]
    {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        let cfg = load_config(&root);
        let (supported, enrolled) = query_capability(&app).await;
        return Ok(build_status(supported, enrolled, &cfg, &lock_state.0));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, lock_state);
        Ok(non_mobile_status())
    }
}

#[tauri::command]
pub(crate) async fn app_security_lock_set<R: Runtime>(
    app: AppHandle<R>,
    lock_state: State<'_, SecurityLockState>,
    enabled: bool,
    skip_setup: Option<bool>,
    title: Option<String>,
    subtitle: Option<String>,
) -> Result<SecurityLockStatus, &'static str> {
    #[cfg(target_os = "android")]
    {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        let current = load_config(&root);
        let first_launch_skip = !enabled && skip_setup.unwrap_or(false) && !current.decided;
        if !first_launch_skip && (enabled || current.enabled) {
            verify_owner(&app, title, subtitle).await?;
        }
        let next = SecurityLockConfig {
            version: 1,
            decided: true,
            enabled,
        };
        save_config(&root, &next)?;
        lock_state.0.store(true, Ordering::SeqCst);
        let (supported, enrolled) = query_capability(&app).await;
        return Ok(build_status(supported, enrolled, &next, &lock_state.0));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, lock_state, enabled, skip_setup, title, subtitle);
        Ok(non_mobile_status())
    }
}

#[tauri::command]
pub(crate) async fn app_security_lock_unlock<R: Runtime>(
    app: AppHandle<R>,
    lock_state: State<'_, SecurityLockState>,
    title: Option<String>,
    subtitle: Option<String>,
) -> Result<SecurityLockStatus, &'static str> {
    #[cfg(target_os = "android")]
    {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        let cfg = load_config(&root);
        if cfg.enabled {
            verify_owner(&app, title, subtitle).await?;
        }
        lock_state.0.store(true, Ordering::SeqCst);
        let (supported, enrolled) = query_capability(&app).await;
        return Ok(build_status(supported, enrolled, &cfg, &lock_state.0));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, lock_state, title, subtitle);
        Ok(non_mobile_status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_lock_config_defaults_and_persists_atomically() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bob-sec-lock-test-{stamp}"));
        let _ = std::fs::remove_dir_all(&dir);
        let initial = load_config(&dir);
        assert_eq!(initial, SecurityLockConfig::default());
        assert!(!initial.decided);
        assert!(!initial.enabled);

        let updated = SecurityLockConfig {
            version: 1,
            decided: true,
            enabled: true,
        };
        save_config(&dir, &updated).unwrap();
        assert_eq!(load_config(&dir), updated);
        assert!(!dir.join("security_lock.json.pending").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

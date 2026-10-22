use tauri::{plugin::{Builder, TauriPlugin}, Runtime};

#[cfg(target_os = "android")]
pub struct DeviceVault<R: Runtime>(tauri::plugin::PluginHandle<R>);

#[cfg(target_os = "android")]
#[derive(serde::Deserialize)]
pub struct UnlockResult { pub key: String, pub envelope: String, pub error: Option<String> }

#[cfg(target_os = "android")]
impl<R: Runtime> DeviceVault<R> {
    /// Must be called from a blocking worker, never Android's UI thread.
    pub fn unlock(&self, envelope: Option<String>) -> Result<UnlockResult, String> {
        #[derive(serde::Serialize)]
        struct Args { envelope: Option<String> }
        self.0.run_mobile_plugin("unlock", Args { envelope })
            .map_err(|_| "REMOTE_SYSTEM_AUTH_FAILED".into())
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("device-vault")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = _api.register_android_plugin("org.bobbik.devicevault", "DeviceVaultPlugin")?;
                _app.manage(DeviceVault(handle));
            }
            Ok(())
        }).build()
}

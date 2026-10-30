//! Native-only Android identity and provider secret vault + HTTPS bridge.
//! No JavaScript-facing credential commands.
use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    AppHandle, Manager, Runtime,
};

const LIMIT: usize = 64 * 1024;
pub(crate) struct AndroidVault<R: Runtime>(PluginHandle<R>);
#[derive(Serialize)]
struct WriteRequest {
    bytes: Vec<u8>,
}
#[derive(Deserialize)]
struct ReadResponse {
    bytes: Option<Vec<u8>>,
}
#[derive(Serialize)]
struct SecretWriteRequest {
    slot: String,
    bytes: Vec<u8>,
}
#[derive(Serialize)]
struct SecretSlotRequest {
    slot: String,
}
#[derive(Serialize)]
pub(crate) struct AndroidHttpRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub timeout_ms: u32,
}
#[derive(Deserialize)]
pub(crate) struct AndroidHttpResponse {
    pub status: u16,
    pub body: String,
}

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("bob-device-vault")
        // Prevent mobile fallback from exposing decrypted credentials to JS.
        .invoke_handler(|invoke| {
            invoke.resolver.reject("NATIVE_ONLY");
            true
        })
        .setup(|app, api| {
            let handle = api.register_android_plugin("org.bobagent.vault", "DeviceVaultPlugin")?;
            app.manage(AndroidVault(handle));
            Ok(())
        })
        .build()
}

pub(crate) async fn store_new<R: Runtime>(
    app: &AppHandle<R>,
    bytes: Vec<u8>,
) -> Result<(), &'static str> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err("VAULT_INVALID_INPUT");
    }
    let handle = app.state::<AndroidVault<R>>().0.clone();
    handle
        .run_mobile_plugin_async::<serde_json::Value>("storeNew", WriteRequest { bytes })
        .await
        .map_err(|_| "VAULT_STORE_FAILED")?;
    Ok(())
}

pub(crate) async fn read<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Vec<u8>>, &'static str> {
    let handle = app.state::<AndroidVault<R>>().0.clone();
    let result: ReadResponse = handle
        .run_mobile_plugin_async("readIdentity", serde_json::json!({}))
        .await
        .map_err(|_| "VAULT_READ_FAILED")?;
    if result
        .bytes
        .as_ref()
        .is_some_and(|b| b.is_empty() || b.len() > LIMIT)
    {
        return Err("VAULT_INVALID_DATA");
    }
    Ok(result.bytes)
}

pub(crate) async fn store_secret<R: Runtime>(
    app: &AppHandle<R>,
    slot: &str,
    bytes: Vec<u8>,
) -> Result<(), &'static str> {
    if slot.trim().is_empty() || slot.len() > 64 || bytes.is_empty() || bytes.len() > LIMIT {
        return Err("VAULT_INVALID_INPUT");
    }
    let handle = app.state::<AndroidVault<R>>().0.clone();
    handle
        .run_mobile_plugin_async::<serde_json::Value>(
            "storeSecret",
            SecretWriteRequest {
                slot: slot.trim().to_owned(),
                bytes,
            },
        )
        .await
        .map_err(|_| "VAULT_STORE_FAILED")?;
    Ok(())
}

pub(crate) async fn read_secret<R: Runtime>(
    app: &AppHandle<R>,
    slot: &str,
) -> Result<Option<Vec<u8>>, &'static str> {
    if slot.trim().is_empty() || slot.len() > 64 {
        return Err("VAULT_INVALID_INPUT");
    }
    let handle = app.state::<AndroidVault<R>>().0.clone();
    let result: ReadResponse = handle
        .run_mobile_plugin_async(
            "readSecret",
            SecretSlotRequest {
                slot: slot.trim().to_owned(),
            },
        )
        .await
        .map_err(|_| "VAULT_READ_FAILED")?;
    if result
        .bytes
        .as_ref()
        .is_some_and(|b| b.is_empty() || b.len() > LIMIT)
    {
        return Err("VAULT_INVALID_DATA");
    }
    Ok(result.bytes)
}

pub(crate) async fn clear_secret<R: Runtime>(
    app: &AppHandle<R>,
    slot: &str,
) -> Result<(), &'static str> {
    if slot.trim().is_empty() || slot.len() > 64 {
        return Err("VAULT_INVALID_INPUT");
    }
    let handle = app.state::<AndroidVault<R>>().0.clone();
    handle
        .run_mobile_plugin_async::<serde_json::Value>(
            "clearSecret",
            SecretSlotRequest {
                slot: slot.trim().to_owned(),
            },
        )
        .await
        .map_err(|_| "VAULT_CLEAR_FAILED")?;
    Ok(())
}

pub(crate) async fn https_request<R: Runtime>(
    app: &AppHandle<R>,
    req: AndroidHttpRequest,
) -> Result<AndroidHttpResponse, &'static str> {
    let handle = app.state::<AndroidVault<R>>().0.clone();
    handle
        .run_mobile_plugin_async("httpsRequest", req)
        .await
        .map_err(|_| "PROVIDER_HTTP_FAILED")
}

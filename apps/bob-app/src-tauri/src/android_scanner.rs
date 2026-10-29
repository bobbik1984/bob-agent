//! Native camera bridge. It does not parse, authenticate, pair or execute a code.
use serde::Deserialize;
use tauri::{plugin::{Builder, PluginHandle, TauriPlugin}, AppHandle, Manager, Runtime};

struct Scanner<R: Runtime>(PluginHandle<R>);
#[derive(Deserialize)]
struct ScanResponse { status: String, code: Option<String> }

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("bob-pairing-scanner")
        .invoke_handler(|invoke| { invoke.resolver.reject("NATIVE_ONLY"); true })
        .setup(|app, api| {
            let handle = api.register_android_plugin("org.bobagent.scanner", "PairingScannerPlugin")?;
            app.manage(Scanner(handle));
            Ok(())
        }).build()
}

pub(crate) async fn scan<R: Runtime>(app: &AppHandle<R>) -> Result<String, &'static str> {
    let handle = app.state::<Scanner<R>>().0.clone();
    let response: ScanResponse = handle.run_mobile_plugin_async("scanPairingCode", serde_json::json!({})).await
        .map_err(|_| "SCAN_UNAVAILABLE")?;
    match response.status.as_str() {
        "ok" => crate::scan_commands::validate(response.code.ok_or("SCAN_INVALID")?),
        "cancelled" => Err("SCAN_CANCELLED"),
        "timeout" => Err("SCAN_TIMEOUT"),
        "permission_denied" => Err("SCAN_PERMISSION_DENIED"),
        "busy" => Err("SCAN_BUSY"),
        "invalid" => Err("SCAN_INVALID"),
        _ => Err("SCAN_UNAVAILABLE"),
    }
}

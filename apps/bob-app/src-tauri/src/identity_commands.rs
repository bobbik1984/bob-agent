//! Public identity IPC. Device ID comes from the existing kernel, never JS.
use crate::{device_identity::{self, PublicIdentity}, with_app, AppState};
use tauri::{AppHandle, Runtime, State};

#[tauri::command]
pub(crate) async fn app_identity_create<R: Runtime>(
    app: AppHandle<R>, state: State<'_, AppState>,
) -> Result<PublicIdentity, &'static str> {
    let device = with_app(&state.0, |a| Ok(a.device_id().to_owned()))?;
    device_identity::create(&app, device).await
}

#[tauri::command]
pub(crate) async fn app_identity_public<R: Runtime>(
    app: AppHandle<R>, state: State<'_, AppState>,
) -> Result<Option<PublicIdentity>, &'static str> {
    let device = with_app(&state.0, |a| Ok(a.device_id().to_owned()))?;
    let identity = device_identity::public(&app).await?;
    if identity.as_ref().is_some_and(|i| i.device_id != device) {
        return Err("IDENTITY_DEVICE_MISMATCH");
    }
    Ok(identity)
}

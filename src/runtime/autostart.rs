use crate::common::error::{Error, Result};
use tauri_plugin_autostart::ManagerExt;

/// Disable development autostart or refresh an enabled release login entry.
pub fn refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>, debug_build: bool) -> Result<()> {
    let manager = app.autolaunch();
    if debug_build {
        return manager.disable().map_err(|error| {
            Error::Other(format!(
                "cannot disable development launch at login: {error}"
            ))
        });
    }
    // Registration records an executable path; moving the app leaves it stale.
    if manager
        .is_enabled()
        .map_err(|error| Error::Other(format!("cannot read launch at login: {error}")))?
    {
        manager
            .enable()
            .map_err(|error| Error::Other(format!("cannot refresh launch at login: {error}")))?;
    }
    Ok(())
}

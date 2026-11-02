use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

use crate::common::error::{Error, Result};

/// Read a cache file without allocating beyond its documented size limit.
pub fn read_bounded(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "cached file exceeds size limit",
        ));
    }
    Ok(bytes)
}

/// Share a process-local mutex for operations on the same path, releasing unused entries.
pub fn path_mutex(path: &Path) -> Result<Arc<Mutex<()>>> {
    static LOCKS: Mutex<BTreeMap<PathBuf, Weak<Mutex<()>>>> = Mutex::new(BTreeMap::new());
    let mut locks = LOCKS
        .lock()
        .map_err(|e| Error::Other(format!("path lock poisoned: {e}")))?;
    locks.retain(|_, lock| lock.strong_count() > 0);
    let lock = locks.get(path).and_then(Weak::upgrade).unwrap_or_default();
    locks.insert(path.into(), Arc::downgrade(&lock));
    Ok(lock)
}

/// Replace a file through a synced temporary sibling and `rename()`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_with_mode(path, bytes, None)
}

/// Atomically replace a private file, using owner-only permissions on Unix.
pub fn atomic_write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_with_mode(path, bytes, Some(0o600))
}

fn atomic_write_with_mode(path: &Path, bytes: &[u8], mode: Option<u32>) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Io(std::io::Error::other("missing parent directory")))?;
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let tmp = parent.join(format!(".tmp-{name}-{}", std::process::id()));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options.open(&tmp)?;
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// SHA-256 in the lowercase hexadecimal format used by widget registries.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Show a native top-level window at the bottom of the z-order without moving,
/// resizing, or focusing it.
#[cfg(windows)]
pub fn reveal_window_at_bottom(hwnd: windows::Win32::Foundation::HWND) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .map_err(|error| Error::Other(format!("cannot reveal window: {error}")))
    }
}

/// Remove the native caption and resize frame from a top-level window.
#[cfg(windows)]
pub fn remove_window_decorations(hwnd: windows::Win32::Foundation::HWND) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_CAPTION, WS_THICKFRAME,
    };

    let decorations = (WS_CAPTION.0 | WS_THICKFRAME.0) as isize;
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !decorations);
    }
    if unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } & decorations != 0 {
        return Err(Error::Other("cannot remove window decorations".to_string()));
    }
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .map_err(|error| Error::Other(format!("cannot refresh window frame: {error}")))
    }
}

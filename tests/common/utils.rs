use aidget::common::utils::atomic_write;
use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use tempfile::TempDir;

#[test]
fn bounded_reads_preserve_exact_limits_and_report_missing_or_oversized_files() {
    use aidget::common::utils::read_bounded;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cache");
    assert_eq!(
        read_bounded(&path, 4).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    fs::write(&path, b"1234").unwrap();
    assert_eq!(read_bounded(&path, 4).unwrap(), b"1234");
    assert_eq!(
        read_bounded(&path, 3).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn path_mutex_serializes_independent_clients_of_the_same_path() {
    use aidget::common::utils::path_mutex;
    let dir = TempDir::new().unwrap();
    let first = path_mutex(&dir.path().join("image")).unwrap();
    let second = path_mutex(&dir.path().join("image")).unwrap();
    let independent = path_mutex(&dir.path().join("other")).unwrap();
    let _guard = first.lock().unwrap();
    assert!(second.try_lock().is_err());
    assert!(independent.try_lock().is_ok());
    drop(_guard);
    assert!(second.try_lock().is_ok());
}

/// Set a file or directory's modified time through a handle with the access
/// rights Windows requires for timestamp updates.
pub(crate) fn set_modified(path: &Path, timestamp: u64) {
    let modified = UNIX_EPOCH + Duration::from_secs(timestamp);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_WRITE_ATTRIBUTES: u32 = 0x0000_0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

        fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }
    #[cfg(not(windows))]
    fs::File::open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

#[test]
fn atomic_write_replaces_and_leaves_no_temp_files() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("board.json");

    atomic_write(&path, b"{\"first\":true}").unwrap();
    atomic_write(&path, b"{\"second\":true}").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"{\"second\":true}");
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| name.starts_with(".tmp-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(leftovers.is_empty());
}

#[cfg(windows)]
struct TestWindow(windows::Win32::Foundation::HWND);

#[cfg(windows)]
impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0);
        }
    }
}

#[cfg(windows)]
fn create_test_window(title: windows::core::PCWSTR) -> TestWindow {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WINDOW_EX_STYLE, WS_POPUP};

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            title,
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    };
    TestWindow(hwnd)
}

#[cfg(windows)]
fn create_decorated_test_window(title: windows::core::PCWSTR) -> TestWindow {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    };

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            title,
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            360,
            220,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    };
    TestWindow(hwnd)
}

#[cfg(windows)]
fn window_is_above(
    upper: windows::Win32::Foundation::HWND,
    lower: windows::Win32::Foundation::HWND,
) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_HWNDNEXT};

    let mut current = unsafe { GetWindow(upper, GW_HWNDNEXT).ok() };
    while let Some(window) = current {
        if window == lower {
            return true;
        }
        current = unsafe { GetWindow(window, GW_HWNDNEXT).ok() };
    }
    false
}

#[cfg(windows)]
#[test]
fn reveal_window_at_bottom_shows_it_below_its_peer() {
    use aidget::common::utils::reveal_window_at_bottom;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SW_SHOWNOACTIVATE,
    };

    let lower = create_test_window(w!("aidget-lower"));
    let peer = create_test_window(w!("aidget-peer"));

    unsafe {
        let _ = ShowWindow(peer.0, SW_SHOWNOACTIVATE);
        SetWindowPos(
            lower.0,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
    }
    assert!(window_is_above(lower.0, peer.0));

    reveal_window_at_bottom(lower.0).unwrap();

    assert!(unsafe { IsWindowVisible(lower.0).as_bool() });
    assert!(window_is_above(peer.0, lower.0));
}

#[cfg(windows)]
#[test]
fn remove_window_decorations_makes_the_client_fill_the_window() {
    use aidget::common::utils::remove_window_decorations;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowLongPtrW, GetWindowRect, GWL_STYLE, WS_CAPTION, WS_THICKFRAME,
    };

    let window = create_decorated_test_window(w!("aidget-decorated"));
    let style = unsafe { GetWindowLongPtrW(window.0, GWL_STYLE) };
    assert_ne!(style & WS_CAPTION.0 as isize, 0);

    remove_window_decorations(window.0).unwrap();

    let style = unsafe { GetWindowLongPtrW(window.0, GWL_STYLE) };
    assert_eq!(style & WS_CAPTION.0 as isize, 0);
    assert_eq!(style & WS_THICKFRAME.0 as isize, 0);
    let mut outer = windows::Win32::Foundation::RECT::default();
    let mut client = windows::Win32::Foundation::RECT::default();
    unsafe {
        GetWindowRect(window.0, &mut outer).unwrap();
        GetClientRect(window.0, &mut client).unwrap();
    }
    assert_eq!(outer.right - outer.left, client.right - client.left);
    assert_eq!(outer.bottom - outer.top, client.bottom - client.top);
}

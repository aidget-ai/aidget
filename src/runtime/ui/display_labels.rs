//! The names frames and settings store for displays.
use tauri::Monitor;

/// The name a frame stores for a monitor at `index`.
///
/// macOS and Windows report a device name. Wayland can report none, and an
/// unnamed monitor must not collapse into `primary`, or every multi-screen
/// frame would be restored onto one screen, so those keep the list position
/// and origin as the identifier.
pub fn display_name(monitor: &Monitor, index: usize) -> String {
    match monitor.name().filter(|name| !name.is_empty()) {
        Some(name) => name.clone(),
        None => {
            let position = monitor.position();
            format!("display-{index}@{}:{}", position.x, position.y)
        }
    }
}

/// The names frames store for `monitors`, in the same order.
pub(super) fn display_names(monitors: &[Monitor]) -> Vec<String> {
    distinct_names(
        monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| display_name(monitor, index))
            .collect(),
    )
}

/// Tell repeated names apart: macOS names a monitor after its model, so twins
/// share one. The first keeps its plain name and saved frames stay valid.
///
/// shortcut: twins are told apart by list order, so they can swap if the system
/// reorders them; use a hardware identifier if that shows up in practice.
pub fn distinct_names(names: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashMap::new();
    names
        .into_iter()
        .map(|name| {
            let count = seen.entry(name.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                name
            } else {
                format!("{name} ({count})")
            }
        })
        .collect()
}

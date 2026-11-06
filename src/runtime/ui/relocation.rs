//! Which displays the names stored in frames refer to.
use tauri::Monitor;

/// The names frames use for connected displays, with the primary's position.
#[derive(Debug, Clone)]
pub struct DisplayNames<'a> {
    pub names: Vec<&'a str>,
    pub primary: Option<usize>,
}

impl<'a> DisplayNames<'a> {
    /// Names of the displays `connected_displays` returned.
    pub(super) fn of(monitors: &'a [(String, Monitor)], primary: Option<usize>) -> Self {
        DisplayNames {
            names: monitors.iter().map(|(name, _)| name.as_str()).collect(),
            primary,
        }
    }

    /// Resolve a stored name; `primary` follows the system's primary display, or
    /// the first one where none is reported, as `physical_geometry` does.
    pub(super) fn index(&self, name: &str) -> Option<usize> {
        if name == "primary" {
            return self.primary.or((!self.names.is_empty()).then_some(0));
        }
        self.names.iter().position(|candidate| *candidate == name)
    }
}

//! Application command inventory shared with the build-time ACL generator.

macro_rules! command_groups {
    ($($group:ident: [$($command:literal),* $(,)?]),* $(,)?) => {
        $(#[doc = concat!("Commands granted to the ", stringify!($group), " window role.")]
        pub const $group: &[&str] = &[$($command),*];)*
        /// Every application command participates in Tauri's default-deny ACL.
        pub const ALL_COMMANDS: &[&str] = &[$($($command),*),*];
    };
}

command_groups! {
    MAIN_COMMANDS: [
        "main_state", "main_painted", "main_widget_cached_preview", "main_widget_details",
        "integration_state", "set_agent_integration", "set_preferences",
        "set_github_token",
        "set_widget_enabled", "delete_widget", "restore_widget",
        "deleted_widget_preview", "purge_deleted_widget", "main_widget_preview",
        "community_list", "community_media", "community_install", "community_downloads",
    ],
    WIDGET_COMMANDS: [
        "widget_state", "widget_loaded", "widget_ready", "persist_window_position",
        "begin_widget_drag", "start_widget_drag", "end_widget_drag", "widget_location",
        "widget_storage_get", "widget_storage_set", "widget_storage_remove", "widget_storage_clear",
    ],
}

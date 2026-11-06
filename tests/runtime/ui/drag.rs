use std::time::{Duration, Instant};

use aidget::runtime::ui::{native_drag_running, records_move, DisplayNames, UiState};

const NAMES: [&str; 1] = ["Monitor #1"];

fn connected() -> DisplayNames<'static> {
    DisplayNames {
        names: NAMES.to_vec(),
        primary: Some(0),
    }
}

#[test]
fn the_system_evacuating_a_missing_display_is_not_recorded() {
    assert!(!records_move("Monitor #2", &connected(), false, false));
}

#[test]
fn dragging_a_widget_off_a_missing_display_is_recorded() {
    assert!(records_move("Monitor #2", &connected(), true, false));
}

#[test]
fn a_window_manager_move_on_a_connected_display_is_recorded() {
    assert!(records_move("Monitor #1", &connected(), false, false));
    assert!(records_move("primary", &connected(), false, false));
}

#[test]
fn a_click_on_the_drag_handle_keeps_the_home_of_a_widget_on_a_stand_in_display() {
    // The widget's display is gone, so it sits where the frame was fitted. A drag
    // session without movement must not turn that spot into its new home.
    assert!(!records_move("Monitor #2", &connected(), true, true));
}

#[test]
fn a_window_that_never_left_its_frame_records_nothing() {
    assert!(!records_move("Monitor #1", &connected(), false, true));
}

#[test]
fn a_window_manager_move_is_recorded_when_no_primary_display_is_reported() {
    let connected = DisplayNames {
        names: NAMES.to_vec(),
        primary: None,
    };

    assert!(records_move("primary", &connected, false, false));
}

#[test]
fn a_native_drag_outlives_its_end_only_outside_macos() {
    let state = UiState::default();
    *state.drag_started.lock().unwrap() = Some(Instant::now());

    assert_eq!(native_drag_running(&state), !cfg!(target_os = "macos"));
}

#[test]
fn a_native_drag_stops_counting_after_it_expires() {
    let state = UiState::default();
    *state.drag_started.lock().unwrap() = Some(Instant::now() - Duration::from_secs(31));

    assert!(!native_drag_running(&state));
}

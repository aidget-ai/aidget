use std::time::Duration;

use aidget::runtime::ui::{
    finish_gesture, gesture_owns, records_move, start_gesture, DisplayNames, UiState,
};

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
fn a_drag_onto_the_stand_in_spot_still_adopts_it() {
    // The system left the window at one spot, and its frame computes to another.
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (200, 100));

    let user_drag = finish_gesture(&state, DRAGGED, (1560, 80));

    assert!(records_move("Monitor #2", &connected(), user_drag, true));
}

#[test]
fn a_click_on_the_stand_in_spot_keeps_the_home() {
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (1560, 80));

    let user_drag = finish_gesture(&state, DRAGGED, (1560, 80));

    assert!(!records_move("Monitor #2", &connected(), user_drag, true));
}

#[test]
fn a_drag_that_reports_no_new_position_keeps_the_home() {
    // Wayland hides global positions, so the window reports the same spot throughout.
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (0, 0));

    let user_drag = finish_gesture(&state, DRAGGED, (0, 0));

    assert!(!records_move("Monitor #2", &connected(), user_drag, false));
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

const DRAGGED: &str = "widget-dragged";
const OTHER: &str = "widget-other";

#[test]
fn a_click_where_the_system_left_a_window_is_not_a_move() {
    // The display just went away, so the window sits at a spot no frame describes.
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (-40, 900));

    assert!(!finish_gesture(&state, DRAGGED, (-40, 900)));
}

#[test]
fn a_drop_away_from_where_the_drag_began_is_a_move() {
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (-40, 900));

    assert!(finish_gesture(&state, DRAGGED, (120, 900)));
}

#[test]
fn a_pixel_of_rounding_is_not_a_move() {
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (100, 100));

    assert!(!finish_gesture(&state, DRAGGED, (101, 99)));
}

#[test]
fn a_drag_does_not_make_another_windows_move_the_users() {
    // The display goes away while one widget is dragged, and the system moves another.
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (0, 0));

    assert!(gesture_owns(&state, DRAGGED));
    assert!(!gesture_owns(&state, OTHER));
}

#[test]
fn a_finished_drag_no_longer_owns_its_window() {
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (0, 0));

    finish_gesture(&state, DRAGGED, (10, 10));

    assert!(!gesture_owns(&state, DRAGGED));
}

#[test]
fn another_windows_end_leaves_the_drag_running() {
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (0, 0));

    assert!(!finish_gesture(&state, OTHER, (10, 10)));
    assert!(gesture_owns(&state, DRAGGED));
}

#[test]
fn a_lost_drag_stops_owning_its_window_when_it_expires() {
    let state = UiState::default();
    start_gesture(&state, DRAGGED, (0, 0));
    state.dragging.lock().unwrap().as_mut().unwrap().started -= Duration::from_secs(31);

    assert!(!gesture_owns(&state, DRAGGED));
}

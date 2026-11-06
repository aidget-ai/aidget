use aidget::runtime::ui::{
    best_display_placement, centered_position, moved_frame, place_frame, place_in_work_area,
    placement_order,
};
use aidget::widgets::model::WidgetFrame;
use tauri::{
    LogicalPosition, LogicalRect, LogicalSize, PhysicalPosition, PhysicalRect, PhysicalSize,
};

fn logical_rect(x: f64, y: f64, width: f64, height: f64) -> LogicalRect<f64, f64> {
    LogicalRect {
        position: LogicalPosition::new(x, y),
        size: LogicalSize::new(width, height),
    }
}

#[test]
fn centers_a_scaled_window_in_the_selected_physical_work_area() {
    let area = PhysicalRect {
        position: PhysicalPosition::new(0, 0),
        size: PhysicalSize::new(3840, 2088),
    };
    assert_eq!(
        centered_position(&area, PhysicalSize::new(1095, 757), 1.0, 1.5),
        PhysicalPosition::new(1098, 476)
    );
}

#[test]
fn centers_in_a_work_area_with_negative_coordinates() {
    let area = PhysicalRect {
        position: PhysicalPosition::new(-1920, 523),
        size: PhysicalSize::new(1920, 1032),
    };
    assert_eq!(
        centered_position(&area, PhysicalSize::new(1643, 1136), 1.5, 1.0),
        PhysicalPosition::new(-1508, 660)
    );
}

#[test]
fn cross_display_frame_is_not_pulled_back() {
    let frame = WidgetFrame {
        display: "primary".to_string(),
        x: 0.0,
        y: -100.0,
        w: 360.0,
        h: 220.0,
        unplaced: false,
    };
    let (x, y) = place_frame(&frame, (0, 0), (3024, 1786), 2.0, true);
    assert_eq!((x, y), (0, -200));
}

#[test]
fn negative_offsets_are_kept_on_a_live_display() {
    let frame = WidgetFrame {
        display: "primary".to_string(),
        x: -1200.0,
        y: -3000.0,
        w: 360.0,
        h: 220.0,
        unplaced: false,
    };
    let (x, y) = place_frame(&frame, (0, 0), (3024, 1786), 2.0, true);
    assert_eq!((x, y), (-2400, -6000));
}

#[test]
fn missing_display_frame_is_clamped_into_fallback() {
    let frame = WidgetFrame {
        display: "Monitor #999".to_string(),
        x: 5000.0,
        y: 80.0,
        w: 360.0,
        h: 220.0,
        unplaced: false,
    };
    let (x, y) = place_frame(&frame, (0, 0), (3024, 1786), 2.0, false);
    assert_eq!((x, y), ((3024.0 - 720.0) as i32, 160));
}

#[test]
fn moving_a_widget_does_not_infer_size_from_stale_native_pixels() {
    let current = WidgetFrame {
        display: "primary".to_string(),
        x: 100.0,
        y: 50.0,
        w: 360.0,
        h: 220.0,
        unplaced: false,
    };

    let moved = moved_frame(
        &current,
        "Monitor #1".to_string(),
        (-100, -1000),
        (-536, -1410),
        1.0,
    );

    assert_eq!(moved.w, current.w);
    assert_eq!(moved.h, current.h);
    assert_eq!((moved.x, moved.y), (436.0, 410.0));
}

#[test]
fn initial_placement_fills_the_first_gap_inside_the_work_area() {
    let area = logical_rect(0.0, 0.0, 1512.0, 893.0);
    let existing = logical_rect(24.0, 24.0, 360.0, 220.0);

    let placed = place_in_work_area(area, LogicalSize::new(400.0, 350.0), &[existing]);

    assert_eq!((placed.position.x, placed.position.y), (400.0, 24.0));
    assert_eq!((placed.size.width, placed.size.height), (400.0, 350.0));
}

#[test]
fn initial_placement_wraps_to_a_visible_row_when_the_first_row_is_full() {
    let area = logical_rect(0.0, 0.0, 800.0, 893.0);
    let existing = [
        logical_rect(24.0, 24.0, 360.0, 220.0),
        logical_rect(400.0, 24.0, 360.0, 220.0),
    ];

    let placed = place_in_work_area(area, LogicalSize::new(360.0, 220.0), &existing);

    assert_eq!((placed.position.x, placed.position.y), (24.0, 260.0));
}

#[test]
fn initial_placement_keeps_the_window_visible_when_the_screen_is_full() {
    let area = logical_rect(0.0, 0.0, 420.0, 260.0);
    let existing = logical_rect(24.0, 24.0, 360.0, 220.0);

    let placed = place_in_work_area(area, LogicalSize::new(360.0, 220.0), &[existing]);

    assert!(placed.position.x >= 0.0);
    assert!(placed.position.y >= 0.0);
    assert!(placed.position.x + placed.size.width <= area.size.width);
    assert!(placed.position.y + placed.size.height <= area.size.height);
}

#[test]
fn initial_placement_uses_another_display_before_shrinking() {
    let displays = [
        logical_rect(0.0, 0.0, 1512.0, 893.0),
        logical_rect(0.0, 0.0, 1920.0, 1080.0),
    ];
    let occupied = [Vec::new(), Vec::new()];

    let (display, placed) =
        best_display_placement(&displays, LogicalSize::new(1000.0, 1000.0), &occupied).unwrap();

    assert_eq!(display, 1);
    assert_eq!((placed.size.width, placed.size.height), (1000.0, 1000.0));
}

const DISPLAYS: [(&str, f64); 3] = [
    ("Monitor #1", 2_000_000.0),
    ("Monitor #2", 4_000_000.0),
    ("Monitor #3", 1_000_000.0),
];

#[test]
fn automatic_order_tries_the_primary_display_then_larger_displays() {
    assert_eq!(placement_order(&DISPLAYS, Some(2), None), [2, 1, 0]);
}

#[test]
fn chosen_display_is_tried_before_the_primary_display() {
    assert_eq!(
        placement_order(&DISPLAYS, Some(0), Some("Monitor #3")),
        [2, 0, 1]
    );
}

#[test]
fn chosen_primary_display_keeps_the_automatic_order() {
    assert_eq!(
        placement_order(&DISPLAYS, Some(0), Some("Monitor #1")),
        [0, 1, 2]
    );
}

#[test]
fn disconnected_chosen_display_keeps_the_automatic_order() {
    assert_eq!(
        placement_order(&DISPLAYS, Some(0), Some("Monitor #9")),
        [0, 1, 2]
    );
}

#[test]
fn full_chosen_display_falls_through_to_the_next_display() {
    let chosen = logical_rect(0.0, 0.0, 800.0, 600.0);
    let next = logical_rect(0.0, 0.0, 1920.0, 1080.0);

    let (display, _) = best_display_placement(
        &[chosen, next],
        LogicalSize::new(360.0, 220.0),
        &[vec![chosen], Vec::new()],
    )
    .unwrap();

    assert_eq!(display, 1);
}

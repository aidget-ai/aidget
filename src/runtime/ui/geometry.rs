//! Display discovery and widget frame geometry.
use tauri::{
    AppHandle, LogicalPosition, LogicalRect, LogicalSize, PhysicalPosition, PhysicalRect,
    PhysicalSize,
};

use crate::common::error::Result;
use crate::runtime::main_window_state;
use crate::widgets::model::{DisplayInfo, WidgetFrame};
use crate::widgets::store::Store;

const PLACEMENT_MARGIN: f64 = 24.0;
const PLACEMENT_GAP: f64 = 16.0;

/// Center a window in a physical work area after scaling it to the target display.
pub fn centered_position(
    area: &PhysicalRect<i32, u32>,
    window_size: PhysicalSize<u32>,
    current_scale: f64,
    target_scale: f64,
) -> PhysicalPosition<i32> {
    let width = (window_size.width as f64 * target_scale / current_scale).round() as i64;
    let height = (window_size.height as f64 * target_scale / current_scale).round() as i64;
    PhysicalPosition::new(
        (i64::from(area.position.x) + (i64::from(area.size.width) - width) / 2) as i32,
        (i64::from(area.position.y) + (i64::from(area.size.height) - height) / 2) as i32,
    )
}

/// Returns the physical position of the widget inside its display's work area.
///
/// Frames are stored relative to one display and may extend beyond its work
/// area, including into negative coordinates. That is a valid cross-display
/// position, so only a frame whose display disappeared is moved back.
pub fn physical_geometry(app: &AppHandle, frame: &WidgetFrame) -> (i32, i32, f64) {
    let monitors = app.available_monitors().unwrap_or_default();
    let primary = app.primary_monitor().ok().flatten();
    let (selected, found) = if frame.display == "primary" {
        (
            primary.clone().or_else(|| monitors.first().cloned()),
            primary.is_some() || !monitors.is_empty(),
        )
    } else {
        match monitors
            .iter()
            .enumerate()
            .find(|(index, monitor)| display_name(monitor, *index) == frame.display)
            .map(|(_, monitor)| monitor.clone())
        {
            Some(monitor) => (Some(monitor), true),
            None => (primary.clone().or_else(|| monitors.first().cloned()), false),
        }
    };

    let Some(monitor) = selected else {
        return (frame.x.round() as i32, frame.y.round() as i32, 1.0);
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (x, y) = place_frame(
        frame,
        (area.position.x, area.position.y),
        (area.size.width, area.size.height),
        scale,
        found,
    );
    (x, y, scale)
}

/// Convert a frame stored relative to `area` into a physical screen position.
/// Test seam for the display-placement arithmetic; the UI calls it directly.
pub fn place_frame(
    frame: &WidgetFrame,
    area_position: (i32, i32),
    area_size: (u32, u32),
    scale: f64,
    display_found: bool,
) -> (i32, i32) {
    let logical_width = area_size.0 as f64 / scale;
    let logical_height = area_size.1 as f64 / scale;
    let area = (logical_width, logical_height, logical_width, logical_height);
    let placed = if display_found {
        frame.fitted(area)
    } else {
        frame.clamped(area)
    };
    (
        area_position.0 + (placed.x * scale).round() as i32,
        area_position.1 + (placed.y * scale).round() as i32,
    )
}

/// Choose the top-left-most visible position for a new widget.
///
/// Candidate positions include the work-area edges and edges around existing
/// windows. If no non-overlapping position exists, the position with the least
/// overlap is used so the new window is still reachable.
pub fn place_in_work_area(
    area: LogicalRect<f64, f64>,
    size: LogicalSize<f64>,
    occupied: &[LogicalRect<f64, f64>],
) -> LogicalRect<f64, f64> {
    let width = size.width.min(area.size.width);
    let height = size.height.min(area.size.height);
    let xs = placement_candidates(
        area.position.x,
        area.size.width,
        width,
        occupied
            .iter()
            .map(|rect| (rect.position.x, rect.size.width)),
    );
    let ys = placement_candidates(
        area.position.y,
        area.size.height,
        height,
        occupied
            .iter()
            .map(|rect| (rect.position.y, rect.size.height)),
    );

    let mut best = LogicalRect {
        position: LogicalPosition::new(
            area.position.x + PLACEMENT_MARGIN.min((area.size.width - width).max(0.0) / 2.0),
            area.position.y + PLACEMENT_MARGIN.min((area.size.height - height).max(0.0) / 2.0),
        ),
        size: LogicalSize::new(width, height),
    };
    let mut least_overlap = f64::INFINITY;
    for y in ys {
        for &x in &xs {
            let candidate = LogicalRect {
                position: LogicalPosition::new(x, y),
                size: LogicalSize::new(width, height),
            };
            let overlap = occupied
                .iter()
                .map(|existing| overlap_area(candidate, *existing))
                .sum();
            if overlap <= f64::EPSILON {
                return candidate;
            }
            if overlap < least_overlap {
                least_overlap = overlap;
                best = candidate;
            }
        }
    }
    best
}

/// Select a display and position for a newly installed widget.
///
/// A full-size zero-overlap placement always wins. Otherwise the candidate
/// with the least overlap wins, preferring an unshrunk widget only after
/// visibility and overlap have been considered.
pub fn best_display_placement(
    areas: &[LogicalRect<f64, f64>],
    size: LogicalSize<f64>,
    occupied_by_display: &[Vec<LogicalRect<f64, f64>>],
) -> Option<(usize, LogicalRect<f64, f64>)> {
    assert_eq!(
        areas.len(),
        occupied_by_display.len(),
        "each display requires its own occupied rectangles"
    );
    let mut best: Option<(usize, LogicalRect<f64, f64>, bool, f64)> = None;
    for (index, (area, occupied)) in areas.iter().zip(occupied_by_display).enumerate() {
        let placed = place_in_work_area(*area, size, occupied);
        let overlap: f64 = occupied
            .iter()
            .map(|existing| overlap_area(placed, *existing))
            .sum();
        let exact_size = placed.size.width == size.width && placed.size.height == size.height;
        if exact_size && overlap <= f64::EPSILON {
            return Some((index, placed));
        }
        let better = best
            .as_ref()
            .is_none_or(|(_, _, best_exact, best_overlap)| {
                let candidate_free = overlap <= f64::EPSILON;
                let best_free = *best_overlap <= f64::EPSILON;
                if candidate_free != best_free {
                    candidate_free
                } else if exact_size != *best_exact {
                    exact_size
                } else {
                    overlap < *best_overlap
                }
            });
        if better {
            best = Some((index, placed, exact_size, overlap));
        }
    }
    best.map(|(index, placed, _, _)| (index, placed))
}

fn placement_candidates(
    area_start: f64,
    area_size: f64,
    item_size: f64,
    occupied: impl Iterator<Item = (f64, f64)>,
) -> Vec<f64> {
    let inset = PLACEMENT_MARGIN.min((area_size - item_size).max(0.0) / 2.0);
    let first = area_start + inset;
    let last = area_start + (area_size - item_size).max(0.0);
    let mut candidates = vec![first, last];
    for (start, size) in occupied {
        for value in [
            start,
            start + size + PLACEMENT_GAP,
            start - item_size - PLACEMENT_GAP,
        ] {
            candidates.push(value.clamp(first, last));
        }
    }
    candidates.sort_by(f64::total_cmp);
    candidates.dedup_by(|left, right| (*left - *right).abs() < 0.001);
    candidates
}

fn overlap_area(left: LogicalRect<f64, f64>, right: LogicalRect<f64, f64>) -> f64 {
    let width = (left.position.x + left.size.width).min(right.position.x + right.size.width)
        - left.position.x.max(right.position.x);
    let height = (left.position.y + left.size.height).min(right.position.y + right.size.height)
        - left.position.y.max(right.position.y);
    width.max(0.0) * height.max(0.0)
}

/// Place a newly installed widget in the first visible free space.
///
/// The primary display is preferred when it has room. Otherwise displays are
/// tried by available area, and an overlapping-but-visible position is used
/// only when no display has enough free space.
pub fn place_new_widget(app: &AppHandle, store: &Store, id: &str) -> Result<()> {
    let meta = store.load_meta(id)?;
    let board = store.load_board()?;
    let initial = board.widgets.get(id).cloned();
    let archived = main_window_state::archived_widgets(store.root(), &store.widget_ids()?)?;
    let monitors = app.available_monitors()?;
    if monitors.is_empty() {
        return Err(crate::common::error::Error::Other(
            "no display is available".to_string(),
        ));
    }
    let primary = app.primary_monitor()?;
    let primary_index = primary.as_ref().and_then(|primary| {
        monitors
            .iter()
            .position(|monitor| same_monitor(monitor, primary))
    });
    let displays: Vec<_> = monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| (display_name(&monitor, index), monitor))
        .collect();
    let mut order: Vec<_> = (0..displays.len()).collect();
    order.sort_by(|&left, &right| {
        let left_primary = Some(left) == primary_index;
        let right_primary = Some(right) == primary_index;
        right_primary.cmp(&left_primary).then_with(|| {
            let left_area = logical_area(&displays[left].1);
            let right_area = logical_area(&displays[right].1);
            right_area.total_cmp(&left_area)
        })
    });

    let mut areas = Vec::with_capacity(order.len());
    let mut occupied_by_display = Vec::with_capacity(order.len());
    for &index in &order {
        let (_, monitor) = &displays[index];
        let work_area = monitor.work_area();
        let scale = monitor.scale_factor();
        let area = LogicalRect {
            position: LogicalPosition::new(0.0, 0.0),
            size: LogicalSize::new(
                work_area.size.width as f64 / scale,
                work_area.size.height as f64 / scale,
            ),
        };
        let occupied: Vec<_> = board
            .widgets
            .iter()
            .filter(|(existing, _)| existing.as_str() != id && !archived.contains(*existing))
            .filter_map(|(_, frame)| frame_rect_on_display(frame, &displays, primary_index, index))
            .collect();
        areas.push(area);
        occupied_by_display.push(occupied);
    }

    let (position, placed) = best_display_placement(
        &areas,
        LogicalSize::new(meta.size.width, meta.size.height),
        &occupied_by_display,
    )
    .ok_or_else(|| crate::common::error::Error::Other("no display is available".to_string()))?;
    let index = order[position];
    let display = if Some(index) == primary_index {
        "primary".to_string()
    } else {
        displays[index].0.clone()
    };
    store.set_initial_frame(
        id,
        initial.as_ref(),
        WidgetFrame {
            display,
            x: placed.position.x,
            y: placed.position.y,
            w: placed.size.width,
            h: placed.size.height,
        },
    )?;
    Ok(())
}

fn logical_area(monitor: &tauri::Monitor) -> f64 {
    let scale = monitor.scale_factor();
    let size = monitor.work_area().size;
    (size.width as f64 / scale) * (size.height as f64 / scale)
}

fn frame_rect_on_display(
    frame: &WidgetFrame,
    displays: &[(String, tauri::Monitor)],
    primary_index: Option<usize>,
    target_index: usize,
) -> Option<LogicalRect<f64, f64>> {
    let (source_index, display_found) = if frame.display == "primary" {
        let fallback = primary_index.or((!displays.is_empty()).then_some(0));
        (fallback, fallback.is_some())
    } else {
        match displays.iter().position(|(name, _)| name == &frame.display) {
            Some(index) => (Some(index), true),
            None => (primary_index.or((!displays.is_empty()).then_some(0)), false),
        }
    };
    let source = &displays[source_index?].1;
    let source_area = source.work_area();
    let source_scale = source.scale_factor();
    let area = (
        source_area.size.width as f64 / source_scale,
        source_area.size.height as f64 / source_scale,
        source_area.size.width as f64 / source_scale,
        source_area.size.height as f64 / source_scale,
    );
    let fitted = if display_found {
        frame.fitted(area)
    } else {
        frame.clamped(area)
    };
    let physical_x = source_area.position.x + (fitted.x * source_scale).round() as i32;
    let physical_y = source_area.position.y + (fitted.y * source_scale).round() as i32;
    let target_area = displays[target_index].1.work_area();
    let target_scale = displays[target_index].1.scale_factor();
    Some(LogicalRect {
        position: LogicalPosition::new(
            (physical_x - target_area.position.x) as f64 / target_scale,
            (physical_y - target_area.position.y) as f64 / target_scale,
        ),
        size: LogicalSize::new(
            fitted.w * source_scale / target_scale,
            fitted.h * source_scale / target_scale,
        ),
    })
}

/// List displays in the coordinate space used by widget frames.
pub fn displays(app: &AppHandle) -> Result<Vec<DisplayInfo>> {
    let monitors = app.available_monitors()?;
    let primary = app.primary_monitor().ok().flatten();
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let area = monitor.work_area();
            let scale = monitor.scale_factor();
            let name = display_name(&monitor, index);
            let primary = primary
                .as_ref()
                .is_some_and(|primary| same_monitor(primary, &monitor));
            DisplayInfo {
                primary,
                name,
                x: area.position.x as f64 / scale,
                y: area.position.y as f64 / scale,
                width: area.size.width as f64 / scale,
                height: area.size.height as f64 / scale,
                scale,
            }
        })
        .collect())
}

/// The name a frame stores for a monitor at `index`.
///
/// macOS and Windows report a device name. Wayland can report none, and an
/// unnamed monitor must not collapse into `primary`, or every multi-screen
/// frame would be restored onto one screen, so those keep the list position
/// and origin as the identifier.
pub fn display_name(monitor: &tauri::Monitor, index: usize) -> String {
    match monitor.name().filter(|name| !name.is_empty()) {
        Some(name) => name.clone(),
        None => {
            let position = monitor.position();
            format!("display-{index}@{}:{}", position.x, position.y)
        }
    }
}

/// Whether two handles describe the same physical display.
pub(super) fn same_monitor(left: &tauri::Monitor, right: &tauri::Monitor) -> bool {
    left.position() == right.position() && left.size() == right.size()
}

/// The stored name of a monitor that is not the primary display.
pub(super) fn named_display(app: &AppHandle, monitor: &tauri::Monitor) -> String {
    let index = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .position(|candidate| same_monitor(candidate, monitor))
        .unwrap_or(0);
    display_name(monitor, index)
}

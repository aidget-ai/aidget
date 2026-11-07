use std::sync::Arc;

use gtk::glib::Propagation;
use gtk::prelude::*;
use tauri::{AppHandle, WebviewWindow};

use super::PointerWindow;
use crate::common::error::{Error, Result};

pub(super) fn attach(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
    pointer: Arc<PointerWindow>,
) -> Result<()> {
    window
        .with_webview(move |webview| {
            let view = webview.inner();
            view.add_events(
                gdk::EventMask::ENTER_NOTIFY_MASK
                    | gdk::EventMask::LEAVE_NOTIFY_MASK
                    | gdk::EventMask::POINTER_MOTION_MASK,
            );

            let enter_app = app.clone();
            let enter_label = label.clone();
            let enter_pointer = pointer.clone();
            view.connect_enter_notify_event(move |widget, event| {
                let (x, y) = point(widget, event.position());
                enter_pointer.record(&enter_app, &enter_label, true, x, y);
                Propagation::Proceed
            });

            let move_app = app.clone();
            let move_label = label.clone();
            let move_pointer = pointer.clone();
            view.connect_motion_notify_event(move |widget, event| {
                let (x, y) = point(widget, event.position());
                move_pointer.record(&move_app, &move_label, true, x, y);
                Propagation::Proceed
            });

            view.connect_leave_notify_event(move |_, _| {
                pointer.record(&app, &label, false, 0.0, 0.0);
                Propagation::Proceed
            });
        })
        .map_err(|error| Error::Other(format!("cannot access widget webview: {error}")))
}

fn point(widget: &impl IsA<gtk::Widget>, (x, y): (f64, f64)) -> (f64, f64) {
    let width = widget.allocated_width().max(1) as f64;
    let height = widget.allocated_height().max(1) as f64;
    (x.clamp(0.0, width), y.clamp(0.0, height))
}

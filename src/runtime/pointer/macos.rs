use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSEvent, NSTrackingArea, NSTrackingAreaOptions, NSView};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize};
use tauri::{AppHandle, WebviewWindow};

use super::PointerWindow;
use crate::common::error::{Error, Result};

thread_local! {
    static OBSERVERS: RefCell<HashMap<String, Retained<PointerObserver>>> =
        RefCell::new(HashMap::new());
}

struct ObserverIvars {
    app: AppHandle,
    label: String,
    pointer: Arc<PointerWindow>,
    view: Retained<NSView>,
    area: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ObserverIvars]
    struct PointerObserver;

    unsafe impl NSObjectProtocol for PointerObserver {}

    impl PointerObserver {
        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            self.follow(event);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.follow(event);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.ivars()
                .pointer
                .record(&self.ivars().app, &self.ivars().label, false, 0.0, 0.0);
        }
    }
);

impl PointerObserver {
    fn new(
        mtm: MainThreadMarker,
        app: AppHandle,
        label: String,
        pointer: Arc<PointerWindow>,
        view: Retained<NSView>,
    ) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(ObserverIvars {
            app,
            label,
            pointer,
            view,
            area: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn install(&self) {
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect
            | NSTrackingAreaOptions::EnabledDuringMouseDrag;
        let rect = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 0.0,
                height: 0.0,
            },
        };
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                rect,
                options,
                Some(self),
                None,
            )
        };
        self.ivars().view.addTrackingArea(&area);
        *self.ivars().area.borrow_mut() = Some(area);
    }

    fn remove(&self) {
        if let Some(area) = self.ivars().area.borrow_mut().take() {
            self.ivars().view.removeTrackingArea(&area);
        }
    }

    fn follow(&self, event: &NSEvent) {
        let view = &self.ivars().view;
        let point = view.convertPoint_fromView(event.locationInWindow(), None);
        let bounds = view.bounds();
        let y = if view.isFlipped() {
            point.y
        } else {
            bounds.size.height - point.y
        };
        let x = point.x.clamp(0.0, bounds.size.width);
        let y = y.clamp(0.0, bounds.size.height);
        self.ivars()
            .pointer
            .record(&self.ivars().app, &self.ivars().label, true, x, y);
    }
}

pub(super) fn attach(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
    pointer: Arc<PointerWindow>,
) -> Result<()> {
    window
        .with_webview(move |webview| {
            let Some(mtm) = MainThreadMarker::new() else {
                eprintln!("aidget: pointer tracker requires the main thread");
                return;
            };
            let Some(view) = (unsafe { Retained::retain(webview.inner().cast::<NSView>()) }) else {
                eprintln!("aidget: widget WKWebView is null");
                return;
            };
            let observer = PointerObserver::new(mtm, app, label.clone(), pointer, view);
            observer.install();
            OBSERVERS.with(|observers| {
                if let Some(previous) = observers.borrow_mut().insert(label, observer) {
                    previous.remove();
                }
            });
        })
        .map_err(|error| Error::Other(format!("cannot access widget webview: {error}")))
}

pub(super) fn detach(window: &WebviewWindow) {
    let label = window.label().to_string();
    OBSERVERS.with(|observers| {
        if let Some(observer) = observers.borrow_mut().remove(&label) {
            observer.remove();
        }
    });
}

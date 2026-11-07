use std::cell::RefCell;
use std::rc::Rc;

use aidget::common::error::Result;
use aidget::runtime::main_startup::{
    prepare_startup_cursor, prime_startup_window, MainStartup, StartupWindow,
};

#[derive(Clone, Default)]
struct TestWindow(Rc<RefCell<Vec<&'static str>>>);

impl StartupWindow for TestWindow {
    fn show_for_startup(&self) -> Result<()> {
        self.0.borrow_mut().push("show");
        Ok(())
    }

    fn ignore_cursor_events(&self, ignore: bool) -> Result<()> {
        assert!(ignore);
        self.0.borrow_mut().push("ignore");
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[test]
fn gtk_is_shown_before_cursor_passthrough_is_enabled() {
    let calls = TestWindow::default();
    prepare_startup_cursor(&calls).unwrap();
    prime_startup_window(&calls).unwrap();

    assert_eq!(*calls.0.borrow(), ["show", "ignore"]);
}

#[cfg(not(target_os = "linux"))]
#[test]
fn staged_window_ignores_cursor_before_it_is_shown() {
    let calls = TestWindow::default();
    prepare_startup_cursor(&calls).unwrap();
    prime_startup_window(&calls).unwrap();

    assert_eq!(*calls.0.borrow(), ["ignore", "show"]);
}

#[test]
fn main_window_reveal_is_a_one_shot_gate() {
    let startup = MainStartup::default();

    assert!(!startup.is_revealed());
    startup.mark_page_loaded();
    assert!(!startup.try_reveal(|| Ok::<_, &str>(())).unwrap());
    startup.mark_main_painted();
    assert!(startup.try_reveal(|| Ok::<_, &str>(())).unwrap());
    assert!(startup.is_revealed());
    assert!(!startup.try_reveal(|| Ok::<_, &str>(())).unwrap());

    startup.reset_reveal();
    startup.mark_main_painted();
    assert!(!startup.try_reveal(|| Ok::<_, &str>(())).unwrap());
    startup.mark_page_loaded();
    assert!(startup.try_reveal(|| Ok::<_, &str>(())).unwrap());
}

#[test]
fn failed_reveal_can_be_retried_after_both_readiness_events() {
    let startup = MainStartup::default();
    startup.mark_page_loaded();
    startup.mark_main_painted();

    assert_eq!(
        startup.try_reveal(|| Err("position failed")),
        Err("position failed")
    );
    assert!(!startup.is_revealed());
    assert_eq!(startup.try_reveal(|| Ok::<_, &str>(())), Ok(true));
    assert!(startup.is_revealed());
}

#[test]
fn concurrent_reveal_does_not_run_or_report_success_while_in_progress() {
    let startup = MainStartup::default();
    startup.mark_page_loaded();
    startup.mark_main_painted();
    std::thread::scope(|scope| {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let startup_ref = &startup;
        let attempt = scope.spawn(move || {
            startup_ref.try_reveal(|| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok::<_, &str>(())
            })
        });
        entered_rx.recv().unwrap();
        assert!(!startup.is_revealed());
        assert_eq!(
            startup.try_reveal(|| panic!("duplicate reveal")),
            Ok::<bool, &str>(false)
        );
        release_tx.send(()).unwrap();
        assert_eq!(attempt.join().unwrap(), Ok(true));
    });
}

#[test]
fn initial_widget_start_and_announcement_are_one_shot() {
    let startup = MainStartup::default();

    assert!(!startup.widgets_started());
    assert!(startup.claim_widgets_start());
    assert!(startup.widgets_started());
    assert!(!startup.claim_widgets_start());

    assert!(startup.claim_widgets_announcement());
    assert!(!startup.claim_widgets_announcement());
}

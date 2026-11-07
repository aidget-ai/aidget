use aidget::common::error::Error;
use aidget::runtime::pointer::{require_script, PointerEvent, PointerPhase, PointerState};
use aidget::widgets::model::WidgetPermission;
use aidget::widgets::store::{Store, WidgetEdit};

#[test]
fn pointer_state_emits_enter_move_and_leave_without_duplicates() {
    let mut state = PointerState::default();

    assert_eq!(
        state.sample(true, 10.0, 20.0),
        Some(PointerEvent {
            phase: PointerPhase::Enter,
            x: 10.0,
            y: 20.0,
        })
    );
    assert_eq!(state.sample(true, 10.0, 20.0), None);
    assert_eq!(
        state.sample(true, 12.5, 22.0),
        Some(PointerEvent {
            phase: PointerPhase::Move,
            x: 12.5,
            y: 22.0,
        })
    );
    assert_eq!(
        state.sample(false, 0.0, 0.0),
        Some(PointerEvent {
            phase: PointerPhase::Leave,
            x: 12.5,
            y: 22.0,
        })
    );
    assert_eq!(state.sample(false, 0.0, 0.0), None);
}

#[test]
fn pointer_state_reset_starts_with_enter_again() {
    let mut state = PointerState::default();
    state.sample(true, 1.0, 2.0);
    state.reset();

    assert_eq!(
        state.sample(true, 3.0, 4.0),
        Some(PointerEvent {
            phase: PointerPhase::Enter,
            x: 3.0,
            y: 4.0,
        })
    );
}

#[test]
fn pointer_tracking_requires_script_permission() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "note",
            "Note",
            WidgetEdit {
                html: Some("<p>note</p>"),
                permissions: Some(&[]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();

    assert!(matches!(
        require_script(&store, "note").unwrap_err(),
        Error::PermissionDenied(_)
    ));

    store
        .update(
            "note",
            1,
            WidgetEdit {
                permissions: Some(&[WidgetPermission::Script]),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    require_script(&store, "note").unwrap();
}

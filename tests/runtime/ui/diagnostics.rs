use aidget::runtime::ui::{DiagnosticInput, Diagnostics};
use aidget::widgets::{model::WidgetMeta, package::HostCompatibility};

fn meta() -> WidgetMeta {
    WidgetMeta::new("clock", "Clock")
}

#[test]
fn diagnostics_are_session_scoped_revision_filtered_and_urls_lose_queries() {
    let mut diagnostics = Diagnostics::default();
    let meta = meta();
    let session = diagnostics.session_id().to_string();
    diagnostics.mark_ready("clock", meta.revision);
    let event = diagnostics.push(
        "clock",
        DiagnosticInput {
            source: aidget::widgets::model::DiagnosticSource::Error,
            revision: meta.revision,
            message: "broken".into(),
            stack: Some("at app.js:1:2".into()),
            filename: Some("aidget://localhost/content/clock/app.js?v=secret#frame".into()),
            line: Some(1),
            column: Some(2),
        },
    );
    assert_eq!(event.sequence, 1);
    assert_eq!(
        event.filename.as_deref(),
        Some("aidget://localhost/content/clock/app.js")
    );
    let compatibility = HostCompatibility::evaluate(&meta.config, "0.1.12", false).unwrap();
    let report = diagnostics
        .report(&meta, Some(meta.revision), 0, 50, true, compatibility)
        .unwrap();
    assert_eq!(report.session_id, session);
    assert!(report.collector_ready);
    assert!(report.running);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.next_after, 1);

    let other = diagnostics
        .report(
            &meta,
            Some(meta.revision + 1),
            0,
            50,
            true,
            report.compatibility,
        )
        .unwrap();
    assert!(other.diagnostics.is_empty());
}

#[test]
fn diagnostics_enforce_the_per_widget_rate_limit_and_remove_with_the_widget() {
    let mut diagnostics = Diagnostics::default();
    let meta = meta();
    for index in 0..25 {
        diagnostics.push(
            "clock",
            DiagnosticInput {
                source: aidget::widgets::model::DiagnosticSource::ConsoleError,
                revision: meta.revision,
                message: index.to_string(),
                stack: None,
                filename: None,
                line: None,
                column: None,
            },
        );
    }
    assert_eq!(diagnostics.summary("clock", meta.revision).count, 20);
    assert_eq!(diagnostics.summary("clock", meta.revision).dropped, 5);
    diagnostics.add_dropped("clock", 7);
    assert_eq!(diagnostics.summary("clock", meta.revision).dropped, 12);
    diagnostics.remove("clock");
    assert_eq!(diagnostics.summary("clock", meta.revision).count, 0);
}

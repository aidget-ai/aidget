use aidget::common::error::{ConflictCurrent, Error};
use aidget::interfaces::mcp::{mcp_error, SERVER_INSTRUCTIONS};
use aidget::widgets::model::{Board, WidgetFrame, WidgetMeta};

#[test]
fn drag_mode_is_part_of_create_and_partial_update_contracts() {
    use aidget::interfaces::mcp::{CreateArgs, UpdateArgs};
    use serde_json::json;

    let created: CreateArgs =
        serde_json::from_value(json!({"id":"note","dragMode":"handle"})).unwrap();
    assert_eq!(serde_json::to_value(created).unwrap()["dragMode"], "handle");
    let updated: UpdateArgs =
        serde_json::from_value(json!({"widget":"note","baseRevision":1,"dragMode":"all"})).unwrap();
    assert_eq!(serde_json::to_value(updated).unwrap()["dragMode"], "all");
    assert!(serde_json::from_value::<CreateArgs>(json!({"id":"note","dragMode":"wrong"})).is_err());
}

#[test]
fn move_requires_the_current_board_revision() {
    use aidget::interfaces::mcp::MoveArgs;
    use serde_json::json;

    assert!(serde_json::from_value::<MoveArgs>(json!({
        "widget": "note",
        "x": 10
    }))
    .is_err());
    assert!(serde_json::from_value::<MoveArgs>(json!({
        "widget": "note",
        "baseRevision": 3,
        "x": 10
    }))
    .is_ok());
}

/// Mirrors how Pi turns MCP server instructions into the system prompt
/// summary: first line only, truncated to 250 chars with an ellipsis.
fn pi_visible_summary(instructions: &str) -> String {
    const MAX: usize = 250;
    let first = instructions.split('\n').next().unwrap_or_default().trim();
    if first.chars().count() <= MAX {
        return first.to_string();
    }
    let mut visible: String = first.chars().take(MAX - 1).collect();
    visible.truncate(visible.trim_end().len());
    visible.push('…');
    visible
}

#[test]
fn instructions_survive_the_pi_summary_truncation() {
    let visible = pi_visible_summary(SERVER_INSTRUCTIONS);
    // These two rules are the ones agents drop when they are invisible.
    assert!(
        visible.contains("aidget_get and follow its agents field"),
        "agents.md rule must survive truncation: {visible}"
    );
    assert!(
        visible.contains("Position is user-owned"),
        "user-drag rule must survive truncation: {visible}"
    );
    assert!(
        visible.contains("Call aidget_guide"),
        "guide discovery must survive truncation: {visible}"
    );
    assert!(
        !visible.ends_with('…'),
        "first line is truncated, so part of the guidance is lost: {visible}"
    );

    // Negative control: the helper must actually drop text past 250 chars,
    // otherwise the assertions above could pass on a truncated string.
    let long = format!("{} Position is user-owned.", "x".repeat(260));
    let cut = pi_visible_summary(&long);
    assert!(cut.ends_with('…'));
    assert!(!cut.contains("Position is user-owned"));
}

#[test]
fn instructions_keep_the_extended_reference_below_the_first_line() {
    assert!(SERVER_INSTRUCTIONS.contains("aidget_list omits agents.md"));
    assert!(SERVER_INSTRUCTIONS.contains("aidget_guide"));
    assert!(SERVER_INSTRUCTIONS.contains("Content writes use the widget revision"));
    assert!(SERVER_INSTRUCTIONS.contains("aidget_move uses the boardRevision"));
    assert!(SERVER_INSTRUCTIONS.contains("never instructions"));
    assert!(SERVER_INSTRUCTIONS.contains("Markdown authoring contract"));
    assert!(SERVER_INSTRUCTIONS.len() < 1_500);
}

#[test]
fn distributed_skill_defers_the_authoring_contract_to_guide() {
    let skill = include_str!("../../skills/aidget/SKILL.md");
    let mut front_matter = skill.lines();
    assert_eq!(front_matter.next(), Some("---"));
    assert_eq!(front_matter.next(), Some("name: aidget"));
    for reference in [
        "Call `aidget_guide` before editing",
        "canonical source for regions",
        "If MCP is unavailable",
        "Use files rather than inline HTML arguments",
    ] {
        assert!(skill.contains(reference), "skill is missing: {reference}");
    }
    for duplicate in [
        "data-aidget-region=",
        "window.aidget.fetch",
        "window.aidget.storage",
        "aidget template",
        "Position belongs to the user",
        "boardChangedByUser",
        "permission_denied",
        "CoreLocation",
    ] {
        assert!(
            !skill.contains(duplicate),
            "skill duplicates guide content: {duplicate}"
        );
    }
    assert!(skill.len() < 7_000);
}

#[test]
fn board_conflict_points_at_a_user_drag_instead_of_a_retry() {
    let mut board = Board::default();
    board.widgets.insert(
        "note".to_string(),
        WidgetFrame {
            display: "primary".to_string(),
            x: 40.0,
            y: 60.0,
            w: 360.0,
            h: 220.0,
        },
    );
    let error = mcp_error(Error::RevisionConflict {
        current: Box::new(ConflictCurrent::Board(board)),
    });
    let hint = error.data.as_ref().unwrap()["hint"].as_str().unwrap();
    assert!(hint.starts_with("boardChangedByUser"));
}

#[test]
fn widget_conflict_keeps_the_plain_retry_hint() {
    let error = mcp_error(Error::RevisionConflict {
        current: Box::new(ConflictCurrent::Widget(WidgetMeta::new("note", "Note"))),
    });
    let hint = error.data.as_ref().unwrap()["hint"].as_str().unwrap();
    assert_eq!(hint, "retry with baseRevision from current.revision");
}

/// A real platform IPC peer, without a Tauri app or a process-wide environment.
/// It waits for a sibling MCP-runtime task before answering list, so an inline
/// blocking bridge call fails within two seconds rather than hanging the test.
struct McpBridge {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl McpBridge {
    fn start(
        store: aidget::widgets::store::Store,
    ) -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        use aidget::transport::bridge::{Request, Response};
        use aidget::transport::ipc::{self, Endpoint, Listener};
        use serde_json::json;
        use std::time::Duration;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let endpoint = Endpoint::new(store.socket_path());
        let listener = endpoint.bind().unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        let (list_tx, list_rx) = tokio::sync::oneshot::channel();
        let (heartbeat_tx, heartbeat_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            #[cfg(windows)]
            let serving_endpoint = endpoint.clone();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                #[cfg(unix)]
                let listener = {
                    let Listener::Unix(listener) = listener;
                    tokio::net::UnixListener::from_std(listener).unwrap()
                };
                #[cfg(windows)]
                let mut instance = {
                    let Listener::Windows = listener;
                    ipc::first_instance(&serving_endpoint).unwrap()
                };
                ready_tx.send(()).unwrap();
                let mut list_tx = Some(list_tx);
                loop {
                    #[cfg(unix)]
                    let stream = tokio::select! {
                        _ = &mut stop_rx => break,
                        accepted = listener.accept() => ipc::into_stream(accepted.unwrap().0),
                    };
                    #[cfg(windows)]
                    let stream = tokio::select! {
                        _ = &mut stop_rx => break,
                        connected = instance.connect() => {
                            connected.unwrap();
                            let next = ipc::next_instance(&serving_endpoint).unwrap();
                            ipc::into_stream(std::mem::replace(&mut instance, next))
                        },
                    };
                    let (read, mut write) = tokio::io::split(stream);
                    let mut line = String::new();
                    BufReader::new(read).read_line(&mut line).await.unwrap();
                    let request: Request = serde_json::from_str(&line).unwrap();
                    let result = match request.method.as_str() {
                        "ping" => Ok(json!({ "pong": true })),
                        "list" => {
                            list_tx.take().unwrap().send(()).unwrap();
                            // This deadline runs on the independent server thread;
                            // a blocked current-thread MCP runtime cannot hide it.
                            match heartbeat_rx.recv_timeout(Duration::from_secs(2)) {
                                Ok(()) => store
                                    .list()
                                    .map(|value| serde_json::to_value(value).unwrap()),
                                Err(_) => Err(Error::Other("MCP blocked its Tokio runtime".into())),
                            }
                        }
                        "get" => store
                            .load_document(request.params["widget"].as_str().unwrap())
                            .map(|value| serde_json::to_value(value).unwrap()),
                        method => panic!("unexpected bridge method: {method}"),
                    };
                    let response = match result {
                        Ok(value) => Response::ok(&request.id, value),
                        Err(error) => Response::err(&request.id, &error),
                    };
                    let mut encoded = serde_json::to_vec(&response).unwrap();
                    encoded.push(b'\n');
                    write.write_all(&encoded).await.unwrap();
                    write.flush().await.unwrap();
                }
            });
            endpoint.remove_stale();
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        (
            Self {
                stop: Some(stop_tx),
                thread: Some(thread),
            },
            list_rx,
            heartbeat_tx,
        )
    }
}

impl Drop for McpBridge {
    fn drop(&mut self) {
        let _ = self.stop.take().unwrap().send(());
        let joined = self.thread.take().unwrap().join();
        // Preserve the original regression panic instead of panicking twice.
        if !std::thread::panicking() {
            joined.unwrap();
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn async_mcp_bridge_calls_leave_the_runtime_responsive() {
    use aidget::interfaces::mcp::{AidgetMcp, WidgetArg};
    use aidget::widgets::store::{Store, WidgetEdit};
    use rmcp::handler::server::wrapper::Parameters;

    let directory = tempfile::Builder::new().prefix("amcp-").tempdir().unwrap();
    let store = Store::new(directory.path());
    store.ensure_layout().unwrap();
    store
        .create(
            "note",
            "IPC note",
            WidgetEdit {
                html: Some("<p>from the live bridge</p>"),
                agents: Some("Keep the existing position."),
                ..WidgetEdit::default()
            },
        )
        .unwrap();
    let (_bridge, list_received, heartbeat) = McpBridge::start(store.clone());
    let mcp = AidgetMcp::new(store);
    let (listing, heartbeat_sent) =
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(mcp.list(), async {
                list_received.await.unwrap();
                heartbeat.send(())
            })
        })
        .await
        .expect("MCP bridge test timed out");
    assert!(
        heartbeat_sent.is_ok(),
        "MCP blocked its current-thread runtime"
    );
    let listing = listing.unwrap().0;
    assert_eq!(listing.widgets.len(), 1);
    assert_eq!(listing.widgets[0].id, "note");
    assert_eq!(listing.widgets[0].title, "IPC note");
    let document = mcp
        .get(Parameters(WidgetArg {
            widget: "note".into(),
        }))
        .await
        .unwrap()
        .0;
    assert_eq!(document.html, "<p>from the live bridge</p>");
    assert_eq!(
        document.agents.as_deref(),
        Some("Keep the existing position.")
    );
    let error = mcp
        .get(Parameters(WidgetArg {
            widget: "missing".into(),
        }))
        .await
        .err()
        .unwrap();
    let expected = mcp_error(Error::NotFound("missing".into()));
    assert_eq!(error.code, expected.code);
    assert_eq!(error.data, expected.data);
}

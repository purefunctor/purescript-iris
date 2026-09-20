use std::fs;
use std::sync::Arc;

use analyzer::position::PositionEncoding;
use async_lsp::ResponseError;
use async_lsp::router::Router;
use building::lifecycle::{
    ContentAuthority, DiskObservation, DocumentKind, ForeignEvent, LifecycleEvent, SourceEvent,
    SourceUnitKey,
};
use configuration::{Configuration, Diagnostics};
use files::ForeignSourceKind;
use iris_build::compilation::{CompilationState, MaterializedPrim};
use lsp_types::{
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, Position, Range,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Url,
};
use serde_json::json;
use tempfile::tempdir;

use super::preparation::{Preparation, PreparationFinished};
use super::workspace::{
    DiagnosticTrigger, PreparedInitialWorkspace, WorkspaceContext, WorkspaceNotification,
};
use super::{
    ConfigurationReceived, SourceMetadata, State, apply_content_changes, document_kind,
    finish_workspace_configuration, finish_workspace_preparation, observe_disk,
    package_source_roots, source_unit_from_document_uri, source_unit_from_foreign_uri,
    source_unit_from_source_uri,
};

fn test_config() -> Arc<Configuration> {
    Arc::new(Configuration {
        diagnostics: Diagnostics { on_open: false, on_save: false, on_change: false },
        ..Configuration::default()
    })
}

fn test_state(config: Arc<Configuration>, client: async_lsp::ClientSocket) -> State {
    let mut state = State::new(
        Arc::clone(&config),
        client,
        "iris-lsp".to_string(),
        "test".to_string(),
        Arc::new(Preparation::new()),
    );
    let prim = MaterializedPrim::new().unwrap();
    let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
    let prepared = PreparedInitialWorkspace { compilation, source_roots: vec![] };
    let pending = state.workspace.install(prepared).unwrap();
    assert!(pending.is_empty());
    state
}

fn apply_event(state: &mut State, event: LifecycleEvent<i32, SourceMetadata>) {
    let _ =
        state.workspace.test_ready_mut().apply_lifecycle_events([event], DiagnosticTrigger::None);
}

fn close_document(state: &mut State, parameters: DidCloseTextDocumentParams) {
    let client = async_lsp::ClientSocket::clone(&state.client);
    let context = WorkspaceContext {
        root: state.protocol.root.as_deref(),
        position_encoding: state.protocol.position_encoding,
    };
    state.workspace.dispatch(WorkspaceNotification::Close(parameters), context, &client).unwrap();
}

fn open_notification(uri: Url, text: &str) -> WorkspaceNotification {
    WorkspaceNotification::Open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri,
            language_id: "purescript".to_string(),
            version: 1,
            text: text.to_string(),
        },
    })
}

#[test]
fn requests_report_content_modified_while_the_workspace_is_loading() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        let error = state
            .spawn(|_| ())
            .expect_err("invariant violated: waiting workspace produced a snapshot");

        assert_eq!(error.code(), async_lsp::ErrorCode::CONTENT_MODIFIED);
        assert_eq!(error.message(), "Workspace is loading");
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn loading_workspace_preserves_notifications_and_rejects_analysis() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        let context = WorkspaceContext { root: None, position_encoding: PositionEncoding::Utf16 };
        state
            .workspace
            .dispatch(
                open_notification(
                    Url::parse("file:///workspace/Main.purs").unwrap(),
                    "module Main where\n",
                ),
                context,
                &state.client,
            )
            .unwrap();
        assert_eq!(state.workspace.test_pending_len(), 1);

        let error = state
            .spawn(|_| ())
            .expect_err("invariant violated: loading workspace produced a snapshot");
        assert_eq!(error.code(), async_lsp::ErrorCode::CONTENT_MODIFIED);
        assert_eq!(error.message(), "Workspace is loading");
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn cancelled_preparation_rejects_its_completion() {
    let preparation = Preparation::new();
    let generation = preparation.test_arm();

    preparation.cancel();

    assert!(matches!(
        preparation.finish_disposition(generation),
        super::preparation::CompletionDisposition::Stale
    ));
}

#[test]
fn installation_is_waiting_only_and_preserves_notification_order() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        let first_uri = Url::parse("file:///workspace/First.purs").unwrap();
        let second_uri = Url::parse("file:///workspace/Second.purs").unwrap();
        let context = WorkspaceContext { root: None, position_encoding: PositionEncoding::Utf16 };
        state
            .workspace
            .dispatch(
                open_notification(Url::clone(&first_uri), "module First where\n"),
                context,
                &state.client,
            )
            .unwrap();
        let context = WorkspaceContext { root: None, position_encoding: PositionEncoding::Utf16 };
        state
            .workspace
            .dispatch(
                open_notification(Url::clone(&second_uri), "module Second where\n"),
                context,
                &state.client,
            )
            .unwrap();

        let prim = MaterializedPrim::new().unwrap();
        let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
        let prepared = PreparedInitialWorkspace { compilation, source_roots: vec![] };
        let pending = state.workspace.install(prepared).unwrap();
        assert!(
            matches!(&pending[0], WorkspaceNotification::Open(parameters) if parameters.text_document.uri == first_uri)
        );
        assert!(
            matches!(&pending[1], WorkspaceNotification::Open(parameters) if parameters.text_document.uri == second_uri)
        );

        let prim = MaterializedPrim::new().unwrap();
        let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
        let prepared = PreparedInitialWorkspace { compilation, source_roots: vec![] };
        assert!(matches!(
            state.workspace.install(prepared),
            Err(super::LspError::WorkspaceAlreadyReady)
        ));
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn stale_configuration_results_leave_waiting_state_unchanged() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        state.protocol.configuration_generation = 2;
        let event =
            ConfigurationReceived { generation: 1, result: Err("stale failure".to_string()) };

        finish_workspace_configuration(&mut state, event).unwrap();
        let event = ConfigurationReceived {
            generation: 1,
            result: Ok(vec![json!({
                "diagnostics": {"onOpen": true}
            })]),
        };
        finish_workspace_configuration(&mut state, event).unwrap();

        assert!(!state.workspace.is_ready());
        assert_eq!(state.workspace.test_pending_len(), 0);
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn failed_initial_configuration_falls_back_and_replays_notifications() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("spago.yaml"),
        "package:\n  name: application\n  dependencies: []\nworkspace: {}\n",
    )
    .unwrap();
    let source_uri = Url::from_file_path(directory.path().join("Queued.purs")).unwrap();
    let config = test_config();
    let root = directory.path().to_path_buf();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        state.protocol.root = Some(root);
        state.protocol.configuration_generation = 1;
        // Preparation is armed rather than started so the fallback stages the
        // startup settings without spawning a real Spago process.
        let generation = state.preparation.test_arm();
        let context = WorkspaceContext {
            root: state.protocol.root.as_deref(),
            position_encoding: PositionEncoding::Utf16,
        };
        state
            .workspace
            .dispatch(
                open_notification(
                    Url::parse("file:///workspace/Unsupported.txt").unwrap(),
                    "not PureScript",
                ),
                context,
                &state.client,
            )
            .unwrap();
        let context = WorkspaceContext {
            root: state.protocol.root.as_deref(),
            position_encoding: PositionEncoding::Utf16,
        };
        state
            .workspace
            .dispatch(
                open_notification(Url::clone(&source_uri), "module Queued where\n"),
                context,
                &state.client,
            )
            .unwrap();
        let settings = json!({
            "diagnostics": {"onOpen": "invalid"}
        });
        let event = ConfigurationReceived { generation: 1, result: Ok(vec![settings]) };

        finish_workspace_configuration(&mut state, event).unwrap();
        assert!(!state.workspace.is_ready());
        assert_eq!(state.workspace.test_pending_len(), 2);

        let prim = MaterializedPrim::new().unwrap();
        let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
        let prepared = PreparedInitialWorkspace { compilation, source_roots: vec![] };
        finish_workspace_preparation(
            &mut state,
            PreparationFinished { generation, result: Ok(prepared) },
        )
        .unwrap();

        {
            let workspace = state.workspace.test_ready();
            let files = workspace.analysis.files.read();
            let file_id = files.source_id(source_uri.as_str()).unwrap();
            assert_eq!(files.source_version(file_id), Some(1));
            assert_eq!(
                workspace.analysis.engine.content(file_id).unwrap().as_ref(),
                "module Queued where\n"
            );
        }
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn failed_preparation_reports_and_rejects_analysis() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        let generation = state.preparation.test_arm();
        let context = WorkspaceContext { root: None, position_encoding: PositionEncoding::Utf16 };
        state
            .workspace
            .dispatch(
                open_notification(
                    Url::parse("file:///workspace/Main.purs").unwrap(),
                    "module Main where\n",
                ),
                context,
                &state.client,
            )
            .unwrap();

        finish_workspace_preparation(
            &mut state,
            PreparationFinished { generation, result: Err(super::LspError::WorkspaceFailed) },
        )
        .unwrap();

        assert!(!state.workspace.is_ready());
        let error = state
            .spawn(|_| ())
            .expect_err("invariant violated: failed workspace produced a snapshot");
        assert_eq!(error.code(), async_lsp::ErrorCode::REQUEST_FAILED);
        assert_eq!(error.message(), "Workspace preparation failed");
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn stale_preparation_completions_are_ignored() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".into(),
            "test".into(),
            Arc::new(Preparation::new()),
        );
        state.preparation.test_arm();
        let prim = MaterializedPrim::new().unwrap();
        let compilation = CompilationState::new(prim, SourceMetadata::Builtin);
        let prepared = PreparedInitialWorkspace { compilation, source_roots: vec![] };

        finish_workspace_preparation(
            &mut state,
            PreparationFinished { generation: 99, result: Ok(prepared) },
        )
        .unwrap();

        assert!(!state.workspace.is_ready());
        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn settings_only_updates_preserve_ready_runtime_identity() {
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = test_state(Arc::clone(&config), client);
        let workspace = state.workspace.test_ready();
        let files = Arc::as_ptr(&workspace.analysis.files);
        let symbols = Arc::as_ptr(&workspace.analysis.workspace_symbols_cache);
        let suggestions = Arc::as_ptr(&workspace.analysis.suggestions_cache);
        let mut updated = Configuration::clone(&config);
        updated.diagnostics.on_open = true;

        assert!(state.workspace.update_configuration(Arc::new(updated)));

        let workspace = state.workspace.test_ready();
        assert!(workspace.configuration.diagnostics.on_open);
        assert_eq!(Arc::as_ptr(&workspace.analysis.files), files);
        assert_eq!(Arc::as_ptr(&workspace.analysis.workspace_symbols_cache), symbols);
        assert_eq!(Arc::as_ptr(&workspace.analysis.suggestions_cache), suggestions);
        Router::<State, ResponseError>::new(state)
    });
}

fn assert_source_close_result(
    source_uri: Url,
    foreign_uri: Url,
    source_authority: Option<ContentAuthority>,
) {
    let unit = source_unit_from_source_uri(&source_uri).unwrap();
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = test_state(Arc::clone(&config), client);
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::Opened {
                text: Arc::from("module Main where\n"),
                version: 1,
                metadata: SourceMetadata::Unmanaged { editable: true },
            },
        };
        apply_event(&mut state, event);
        let event = LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(&unit),
            kind: ForeignSourceKind::JavaScript,
            event: ForeignEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("export const life = 42;\n")),
            },
        };
        apply_event(&mut state, event);

        let parameters = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: Url::clone(&source_uri) },
        };
        close_document(&mut state, parameters);

        {
            let workspace = state.workspace.test_ready();
            let files = workspace.analysis.files.read();
            assert_eq!(files.source_authority(&unit), source_authority);
            assert_eq!(files.foreign_id(foreign_uri.as_str()), None);
        }

        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn source_and_foreign_uris_produce_the_same_unit_key() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("Source Files").join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    let jsx_path = source_path.with_extension("jsx");
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(foreign_path).unwrap();
    let jsx_uri = Url::from_file_path(jsx_path).unwrap();

    let from_source = source_unit_from_source_uri(&source_uri).unwrap();
    let from_foreign = source_unit_from_foreign_uri(&foreign_uri).unwrap();
    let from_jsx = source_unit_from_foreign_uri(&jsx_uri).unwrap();

    assert_eq!(from_source, from_foreign);
    assert_eq!(from_source, from_jsx);
    assert_eq!(from_source.source(), source_uri.as_str());
    assert_eq!(from_source.foreign(), foreign_uri.as_str());
    assert_eq!(from_source.foreign_for(ForeignSourceKind::Jsx), jsx_uri.as_str());
}

#[test]
fn localhost_source_and_foreign_uris_keep_the_same_authority() {
    let source_uri =
        Url::parse("file://localhost/workspace/Source%20Files/Main.purs?view=1#selection").unwrap();
    let foreign_uri =
        Url::parse("file://localhost/workspace/Source%20Files/Main.js?view=1#selection").unwrap();

    let from_source = source_unit_from_source_uri(&source_uri).unwrap();
    let from_foreign = source_unit_from_foreign_uri(&foreign_uri).unwrap();

    assert_eq!(from_source, from_foreign);
    assert_eq!(from_source.source(), source_uri.as_str());
    assert_eq!(from_source.foreign(), foreign_uri.as_str());
}

#[test]
fn non_file_document_uris_are_rejected() {
    let source_uri = Url::parse("untitled:Main.purs").unwrap();
    assert!(source_unit_from_source_uri(&source_uri).is_err());
}

#[test]
fn document_kind_is_bounded_to_source_and_foreign_extensions() {
    let source_uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let foreign_uri = Url::parse("file:///workspace/Main.js").unwrap();
    let jsx_uri = Url::parse("file:///workspace/Main.jsx").unwrap();
    let unsupported_uri = Url::parse("file:///workspace/Main.json").unwrap();

    assert_eq!(document_kind(&source_uri), Some(DocumentKind::Source));
    assert_eq!(
        document_kind(&foreign_uri),
        Some(DocumentKind::Foreign(ForeignSourceKind::JavaScript))
    );
    assert_eq!(document_kind(&jsx_uri), Some(DocumentKind::Foreign(ForeignSourceKind::Jsx)));
    assert_eq!(document_kind(&unsupported_uri), None);
    assert!(source_unit_from_document_uri(&unsupported_uri).is_err());
}

#[test]
fn closing_a_deleted_source_also_removes_its_deleted_disk_foreign() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(foreign_path).unwrap();
    assert_source_close_result(source_uri, foreign_uri, None);
}

#[test]
fn failed_source_reload_still_removes_its_deleted_disk_foreign() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    fs::write(&source_path, [0xff]).unwrap();
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(foreign_path).unwrap();
    assert_source_close_result(source_uri, foreign_uri, Some(ContentAuthority::Retained));
}

#[test]
fn duplicate_source_close_does_not_reconcile_foreign() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let foreign_path = source_path.with_extension("js");
    fs::write(&source_path, "module Main where\n").unwrap();
    fs::write(&foreign_path, "export const life = 42;\n").unwrap();
    let source_uri = Url::from_file_path(source_path).unwrap();
    let foreign_uri = Url::from_file_path(&foreign_path).unwrap();
    let unit = source_unit_from_source_uri(&source_uri).unwrap();
    let config = test_config();
    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = test_state(Arc::clone(&config), client);
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::Opened {
                text: Arc::from("module Main where\n"),
                version: 1,
                metadata: SourceMetadata::Unmanaged { editable: true },
            },
        };
        apply_event(&mut state, event);
        let event = LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(&unit),
            kind: ForeignSourceKind::JavaScript,
            event: ForeignEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("export const life = 42;\n")),
            },
        };
        apply_event(&mut state, event);

        let parameters = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: Url::clone(&source_uri) },
        };
        close_document(&mut state, parameters);
        fs::remove_file(foreign_path).unwrap();

        let workspace = state.workspace.test_ready();
        let source_id = workspace.analysis.files.read().source_id(source_uri.as_str()).unwrap();
        let foreign_id = workspace.analysis.files.read().foreign_id(foreign_uri.as_str()).unwrap();
        let parameters = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: Url::clone(&source_uri) },
        };
        close_document(&mut state, parameters);

        {
            let workspace = state.workspace.test_ready();
            let files = workspace.analysis.files.read();
            assert_eq!(files.source_id(source_uri.as_str()), Some(source_id));
            assert_eq!(files.foreign_id(foreign_uri.as_str()), Some(foreign_id));
            assert_eq!(workspace.analysis.engine.foreign_file(source_id), Some(foreign_id));
            assert_eq!(
                workspace.analysis.engine.foreign_content(foreign_id).unwrap().as_ref(),
                "export const life = 42;\n",
            );
        }

        Router::<State, ResponseError>::new(state)
    });
}

#[test]
fn disk_observation_distinguishes_content_and_absence() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("Main.purs");
    let source_uri = Url::from_file_path(&source_path).unwrap();

    fs::write(&source_path, "module Main where\n").unwrap();
    assert!(matches!(
        observe_disk(&source_uri),
        DiskObservation::Found(content) if content.as_ref() == "module Main where\n"
    ));

    fs::remove_file(source_path).unwrap();
    assert_eq!(observe_disk(&source_uri), DiskObservation::NotFound);
}

#[cfg(unix)]
#[test]
fn package_roots_include_canonical_symlink_aliases() {
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    let directory = tempdir().unwrap();
    let package_directory = directory.path().join("package");
    let linked_directory = directory.path().join("linked-package");
    fs::create_dir(&package_directory).unwrap();
    symlink(&package_directory, &linked_directory).unwrap();
    let package = iris_build::DiscoveredPackage {
        name: "linked-package".into(),
        files: vec![],
        dependencies: vec![],
        editable: true,
        roots: vec![PathBuf::from("linked-package")],
    };

    let roots = package_source_roots(directory.path(), directory.path(), &package).unwrap();
    let canonical = dunce::canonicalize(package_directory).unwrap();
    assert!(roots.iter().any(|root| root.path == canonical));
}

#[test]
fn incremental_content_changes_apply_sequentially() {
    let uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let changes = [
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(1, 0), Position::new(1, 4))),
            range_length: Some(4),
            text: "answer".to_string(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(1, 9), Position::new(1, 10))),
            range_length: Some(1),
            text: "42".to_string(),
        },
    ];

    let content = apply_content_changes(
        &uri,
        "module Main where\nlife = 0\n",
        &changes,
        PositionEncoding::Utf16,
    )
    .unwrap();

    assert_eq!(content.as_ref(), "module Main where\nanswer = 42\n");
}

#[test]
fn incremental_content_changes_use_negotiated_position_encoding() {
    let uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let changes = [TextDocumentContentChangeEvent {
        range: Some(Range::new(Position::new(0, 3), Position::new(0, 4))),
        range_length: Some(1),
        text: "c".to_string(),
    }];

    let content = apply_content_changes(&uri, "a😀b", &changes, PositionEncoding::Utf16).unwrap();

    assert_eq!(content.as_ref(), "a😀c");
}

#[test]
fn full_content_change_resets_incremental_change_base() {
    let uri = Url::parse("file:///workspace/Main.purs").unwrap();
    let changes = [
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "life = 1".to_string(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 7), Position::new(0, 8))),
            range_length: Some(1),
            text: "2".to_string(),
        },
    ];

    let content =
        apply_content_changes(&uri, "discarded", &changes, PositionEncoding::Utf16).unwrap();

    assert_eq!(content.as_ref(), "life = 2");
}

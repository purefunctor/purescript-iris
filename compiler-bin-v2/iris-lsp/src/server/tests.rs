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
use lsp_types::{
    DidCloseTextDocumentParams, Position, Range, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, Url,
};
use tempfile::tempdir;

use super::{
    State, apply_content_changes, apply_lifecycle_event, did_close, document_kind,
    materialize_prim, observe_disk, source_unit_from_document_uri, source_unit_from_foreign_uri,
    source_unit_from_source_uri,
};

fn test_config() -> Arc<Configuration> {
    Arc::new(Configuration {
        diagnostics: Diagnostics { on_open: false, on_save: false, on_change: false },
        ..Configuration::default()
    })
}

fn assert_source_close_result(
    source_uri: Url,
    foreign_uri: Url,
    source_authority: Option<ContentAuthority>,
) {
    let unit = source_unit_from_source_uri(&source_uri).unwrap();
    let config = test_config();
    let prim_directory = Arc::new(materialize_prim().unwrap());

    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".to_string(),
            "test".to_string(),
            Arc::clone(&prim_directory),
        );
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::Opened {
                text: Arc::from("module Main where\n"),
                version: 1,
                metadata: true,
            },
        };
        apply_lifecycle_event(&mut state, event);
        let event = LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(&unit),
            kind: ForeignSourceKind::JavaScript,
            event: ForeignEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("export const life = 42;\n")),
            },
        };
        apply_lifecycle_event(&mut state, event);

        let parameters = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: Url::clone(&source_uri) },
        };
        did_close(&mut state, parameters).unwrap();

        let files = state.files.read();
        assert_eq!(files.source_authority(&unit), source_authority);
        assert_eq!(files.foreign_id(foreign_uri.as_str()), None);
        drop(files);

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
    let prim_directory = Arc::new(materialize_prim().unwrap());

    let (_server, _) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = State::new(
            Arc::clone(&config),
            client,
            "iris-lsp".to_string(),
            "test".to_string(),
            Arc::clone(&prim_directory),
        );
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::Opened {
                text: Arc::from("module Main where\n"),
                version: 1,
                metadata: true,
            },
        };
        apply_lifecycle_event(&mut state, event);
        let event = LifecycleEvent::Foreign {
            unit: SourceUnitKey::clone(&unit),
            kind: ForeignSourceKind::JavaScript,
            event: ForeignEvent::DiskObserved {
                disk: DiskObservation::Found(Arc::from("export const life = 42;\n")),
            },
        };
        apply_lifecycle_event(&mut state, event);

        let parameters = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: Url::clone(&source_uri) },
        };
        did_close(&mut state, parameters).unwrap();
        fs::remove_file(foreign_path).unwrap();

        let source_id = state.files.read().source_id(source_uri.as_str()).unwrap();
        let foreign_id = state.files.read().foreign_id(foreign_uri.as_str()).unwrap();
        let parameters = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: Url::clone(&source_uri) },
        };
        did_close(&mut state, parameters).unwrap();

        let files = state.files.read();
        assert_eq!(files.source_id(source_uri.as_str()), Some(source_id));
        assert_eq!(files.foreign_id(foreign_uri.as_str()), Some(foreign_id));
        assert_eq!(state.engine.foreign_file(source_id), Some(foreign_id));
        assert_eq!(
            state.engine.foreign_content(foreign_id).unwrap().as_ref(),
            "export const life = 42;\n",
        );
        drop(files);

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

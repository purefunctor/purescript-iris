use std::fs;
use std::ops::Deref;
use std::time::Duration;

use configuration::{Configuration, SourceDiscovery};
use iris_workspace::testing::{Hooks, Point};
use iris_workspace::{
    AnalysisStamp, Command, ConfigurationInput, Document, Event, EventReceiver, InputFailure,
    InputSequence, LanguageServer, Options, Outcome, Reply, Request, RequestFailure, Status,
    Workspace, WorkspaceJoin, WorkspaceSession,
};
use lsp_types::{
    CompletionItem, CompletionResponse, DocumentSymbolResponse, Hover, HoverContents, Position,
    Range, TextDocumentContentChangeEvent, Url,
};
use tempfile::TempDir;

const ORIGINAL: &str = "module Main where\n\nvalue :: Int\nvalue = 1\n\nuse = value\n";
const CHANGED: &str =
    "module Main where\n\nchanged :: String\nchanged = \"hello\"\n\nuse = changed\n";

struct Harness {
    workspace: Workspace,
    join: Option<WorkspaceJoin>,
    events: EventReceiver,
    hooks: Hooks,
    directory: TempDir,
    uri: Url,
}

impl Deref for Harness {
    type Target = Workspace;

    fn deref(&self) -> &Workspace {
        &self.workspace
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.join().unwrap();
        }
    }
}

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(15), future)
        .await
        .expect("workspace sequence timed out")
}

impl Harness {
    fn new(options: Options) -> Harness {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("Main.purs"), ORIGINAL).unwrap();
        let uri = Url::from_file_path(directory.path().join("Main.purs")).unwrap();

        let hooks = Hooks::default();
        let WorkspaceSession { workspace, events, join } =
            Workspace::start_with_hooks(options, Hooks::clone(&hooks)).unwrap();

        Harness { workspace, join: Some(join), events, hooks, directory, uri }
    }

    fn configuration(&self) -> ConfigurationInput {
        let sources = SourceDiscovery::Command {
            program: "node".into(),
            arguments: vec!["-e".into(), "console.log('*.purs')".into()],
        };

        ConfigurationInput {
            root: self.directory.path().to_path_buf(),
            settings: Configuration { sources, ..Configuration::default() },
        }
    }

    fn configure(&self) -> InputSequence {
        self.send(Command::Configure(self.configuration())).unwrap()
    }

    async fn next(&mut self) -> Event {
        bounded(async {
            loop {
                let delivery = self.events.recv().await.expect("workspace event stream closed");
                if let Ok(event) = delivery.release() {
                    return event;
                }
            }
        })
        .await
    }

    async fn ready(&mut self, sequence: InputSequence) -> AnalysisStamp {
        loop {
            match self.status() {
                Status::Ready { stamp, .. } if stamp.revision >= sequence => return stamp,
                Status::Failed { message, .. } => panic!("workspace failed: {message}"),
                _ => {
                    self.next().await;
                }
            }
        }
    }

    fn open(&self, text: &str, version: i32) -> InputSequence {
        self.send(Command::Document(Document::Open {
            uri: Url::clone(&self.uri),
            text: text.into(),
            version,
        }))
        .unwrap()
    }

    fn change(&self, text: &str, version: i32) -> InputSequence {
        let change =
            TextDocumentContentChangeEvent { range: None, range_length: None, text: text.into() };

        self.send(Command::Document(Document::Change {
            uri: Url::clone(&self.uri),
            version,
            changes: vec![change],
        }))
        .unwrap()
    }

    fn hover_request(&self) -> Request<Option<Hover>> {
        let (reply, request) = Reply::channel();
        self.send(Command::LanguageServer(LanguageServer::Hover {
            uri: Url::clone(&self.uri),
            position: Position::new(5, 7),
            reply,
        }))
        .unwrap();

        request
    }

    async fn hover(&self) -> String {
        let hover =
            bounded(self.hover_request()).await.unwrap().release().unwrap().unwrap().unwrap();

        match hover.contents {
            HoverContents::Markup(markup) => markup.value,
            other => format!("{other:?}"),
        }
    }

    async fn symbols(&self) -> Vec<String> {
        let (reply, request) = Reply::channel();
        self.send(Command::LanguageServer(LanguageServer::DocumentSymbols {
            uri: Url::clone(&self.uri),
            reply,
        }))
        .unwrap();

        match bounded(request).await.unwrap().release().unwrap().unwrap().unwrap() {
            DocumentSymbolResponse::Flat(symbols) => {
                symbols.into_iter().map(|symbol| symbol.name).collect()
            }
            DocumentSymbolResponse::Nested(symbols) => {
                symbols.into_iter().map(|symbol| symbol.name).collect()
            }
        }
    }

    async fn completion(&self) -> Vec<CompletionItem> {
        let (reply, request) = Reply::channel();
        self.send(Command::LanguageServer(LanguageServer::Completion {
            uri: Url::clone(&self.uri),
            position: Position::new(5, 9),
            reply,
        }))
        .unwrap();

        match bounded(request).await.unwrap().release().unwrap().unwrap().unwrap() {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        }
    }

    async fn resolve(&self, item: CompletionItem) -> CompletionItem {
        let (reply, request) = Reply::channel();
        self.send(Command::LanguageServer(LanguageServer::ResolveCompletion { item, reply }))
            .unwrap();

        bounded(request).await.unwrap().release().unwrap().unwrap()
    }
}

#[tokio::test]
async fn startup_reconciles_edits_admitted_before_ready() {
    let mut harness = Harness::new(Options::default());
    let mut acknowledgement = harness.hooks.pause_next(Point::BeforeAcknowledgement);

    harness.configure();
    bounded(acknowledgement.entered()).await;

    harness.open(ORIGINAL, 1);
    let sequence = harness.change(CHANGED, 2);

    let (reply, request) = Reply::channel();
    let result = harness.send(Command::LanguageServer(LanguageServer::Hover {
        uri: Url::clone(&harness.uri),
        position: Position::new(5, 7),
        reply,
    }));

    assert!(matches!(result, Err(RequestFailure::Unavailable)));
    assert!(matches!(bounded(request).await, Err(RequestFailure::Unavailable)));

    drop(acknowledgement);
    assert_eq!(harness.ready(sequence).await.revision, sequence);
    assert!(harness.hover().await.contains("String"));

    let symbols = harness.symbols().await;
    assert!(symbols.contains(&"changed".into()));
    assert!(!symbols.contains(&"value".into()));
}

#[tokio::test]
async fn supersession_and_failed_rebuild_preserve_buffers_without_rollback() {
    let mut harness = Harness::new(Options::default());
    let initial = harness.configure();
    let first = harness.ready(initial).await;

    harness.open(CHANGED, 11);
    let mut preparation = harness.hooks.pause_next(Point::BeforePreparation);
    harness.send(Command::Reload).unwrap();
    bounded(preparation.entered()).await;

    let mut invalid = harness.configuration();
    invalid.settings.sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec![
            "-e".into(),
            "process.stderr.write('expected failure'); process.exit(7)".into(),
        ],
    };
    harness.send(Command::Configure(invalid)).unwrap();
    drop(preparation);

    loop {
        if let Event::StatusChanged(Status::Failed { message, .. }) = harness.next().await {
            assert!(message.contains("7"), "{message}");
            break;
        }
    }
    assert!(matches!(harness.status(), Status::Failed { .. }));

    harness.change(ORIGINAL, 12);
    let sequence = harness.configure();
    let recovered = harness.ready(sequence).await;

    assert_ne!(first.incarnation, recovered.incarnation);
    assert!(harness.hover().await.contains("Int"));
}

#[tokio::test]
async fn held_replies_and_completion_tokens_are_invalidated_by_edits_and_rebuilds() {
    let mut harness = Harness::new(Options::default());
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let held = bounded(harness.hover_request()).await.unwrap();
    let completions = harness.completion().await;
    let item = completions
        .into_iter()
        .find(|item| item.data.is_some())
        .expect("completion needs an opaque resolve token");
    assert!(item.data.as_ref().unwrap().is_string());

    let sequence = harness.open(CHANGED, 1);
    assert!(matches!(held.release(), Err(RequestFailure::Stale | RequestFailure::Cancelled)));
    harness.ready(sequence).await;

    let resolved = harness.resolve(CompletionItem::clone(&item)).await;
    assert!(resolved.data.is_none());
    assert_eq!(resolved.documentation, item.documentation);

    let sequence = harness.send(Command::Reload).unwrap();
    harness.ready(sequence).await;

    let resolved = harness.resolve(item).await;
    assert!(resolved.data.is_none());

    let forged = CompletionItem {
        label: "forged".into(),
        data: Some(serde_json::json!({"TermItem": [4294967295u32, 4294967295u32]})),
        ..CompletionItem::default()
    };
    assert!(harness.resolve(forged).await.data.is_none());
}

#[tokio::test]
async fn overload_and_request_cancellation_do_not_block_inputs() {
    let mut harness = Harness::new(Options { request_capacity: 1, ..Options::default() });
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let mut analysis = harness.hooks.pause_next(Point::BeforeAnalysis);
    let request = harness.hover_request();
    bounded(analysis.entered()).await;

    let (reply, rejected) = Reply::channel();
    let result = harness.send(Command::LanguageServer(LanguageServer::Hover {
        uri: Url::clone(&harness.uri),
        position: Position::new(5, 7),
        reply,
    }));

    assert!(matches!(result, Err(RequestFailure::Busy)));
    assert!(matches!(bounded(rejected).await, Err(RequestFailure::Busy)));

    request.cancellation().cancel();
    assert!(matches!(bounded(request).await, Err(RequestFailure::Cancelled)));

    let (reply, rejected) = Reply::channel();
    let command = LanguageServer::DocumentSymbols { uri: Url::clone(&harness.uri), reply };
    assert!(matches!(harness.send(Command::LanguageServer(command)), Err(RequestFailure::Busy)));
    assert!(matches!(bounded(rejected).await, Err(RequestFailure::Busy)));

    let sequence = harness.open(CHANGED, 1);
    drop(analysis);
    harness.ready(sequence).await;
    assert!(harness.hover().await.contains("String"));
}

#[tokio::test]
async fn computed_rename_rejections_are_fenced_until_publication() {
    let mut harness = Harness::new(Options::default());
    let sequence = harness.configure();
    harness.ready(sequence).await;

    for replacement in [
        None,
        Some(Command::Document(Document::Open {
            uri: Url::clone(&harness.uri),
            text: ORIGINAL.into(),
            version: 1,
        })),
        Some(Command::Configure(harness.configuration())),
    ] {
        let (reply, request) = Reply::channel();
        let command = LanguageServer::Rename {
            uri: Url::clone(&harness.uri),
            position: Position::new(5, 7),
            new_name: "use".into(),
            reply,
        };
        harness.send(Command::LanguageServer(command)).unwrap();
        let held = bounded(request).await.unwrap();

        if let Some(command) = replacement {
            let sequence = harness.send(command).unwrap();
            assert!(matches!(
                held.release(),
                Err(RequestFailure::Stale | RequestFailure::Cancelled)
            ));
            harness.ready(sequence).await;
        } else {
            assert!(matches!(
                held.release().unwrap(),
                Err(RequestFailure::LanguageServer(
                    iris_workspace::LanguageServerFailure::RenameRejected(_)
                ))
            ));
        }
    }
}

#[tokio::test]
async fn sequential_unicode_edits_are_atomic_and_versions_reset_only_on_reopen() {
    let mut harness = Harness::new(Options::default());
    harness.configure();
    let text = "module Main where\nvalue = \"a😀b\"\n";
    let sequence = harness.open(text, 7);
    harness.ready(sequence).await;

    let edit = |start, end, text: &str| TextDocumentContentChangeEvent {
        range: Some(Range::new(Position::new(1, start), Position::new(1, end))),
        range_length: None,
        text: text.into(),
    };
    let command = Document::Change {
        uri: Url::clone(&harness.uri),
        version: 8,
        changes: vec![edit(10, 12, "xyz"), edit(8, 15, "42")],
    };
    let sequence = harness.send(Command::Document(command)).unwrap();
    harness.ready(sequence).await;

    let (reply, request) = Reply::channel();
    let command = LanguageServer::Hover {
        uri: Url::clone(&harness.uri),
        position: Position::new(1, 1),
        reply,
    };
    harness.send(Command::LanguageServer(command)).unwrap();

    let hover = bounded(request).await.unwrap().release().unwrap().unwrap().unwrap();
    assert!(format!("{:?}", hover.contents).contains("Int"));

    let command = Document::Change {
        uri: Url::clone(&harness.uri),
        version: 9,
        changes: vec![edit(0, 5, "broken"), edit(8, 1, "invalid")],
    };
    let sequence = harness.send(Command::Document(command)).unwrap();

    loop {
        if matches!(
            harness.next().await,
            Event::InputRejected { failure: InputFailure::InvalidRange(_), .. }
        ) {
            break;
        }
    }
    harness.ready(sequence).await;
    assert!(harness.symbols().await.contains(&"value".into()));

    let sequence = harness.change(CHANGED, 8);
    loop {
        if matches!(
            harness.next().await,
            Event::InputRejected { failure: InputFailure::StaleVersion(_), .. }
        ) {
            break;
        }
    }
    harness.ready(sequence).await;
    assert!(!harness.symbols().await.contains(&"changed".into()));

    harness.send(Command::Document(Document::Close(Url::clone(&harness.uri)))).unwrap();
    let sequence = harness.open(CHANGED, 1);
    harness.ready(sequence).await;
    assert!(harness.hover().await.contains("String"));
}

#[tokio::test]
async fn disk_changes_do_not_replace_open_authority_and_close_restores_disk() {
    let mut harness = Harness::new(Options::default());
    harness.configure();
    let sequence = harness.open(CHANGED, 1);
    harness.ready(sequence).await;

    fs::write(harness.directory.path().join("Main.purs"), ORIGINAL).unwrap();
    let sequence = harness.send(Command::FilesChanged(vec![Url::clone(&harness.uri)])).unwrap();
    harness.ready(sequence).await;
    assert!(harness.hover().await.contains("String"));

    let sequence =
        harness.send(Command::Document(Document::Close(Url::clone(&harness.uri)))).unwrap();
    harness.ready(sequence).await;
    assert!(harness.hover().await.contains("Int"));
}

#[tokio::test]
async fn diagnostics_are_cleared_on_rebuild_and_old_publications_cannot_escape() {
    let mut harness = Harness::new(Options::default());
    fs::write(
        harness.directory.path().join("Main.purs"),
        "module Main where\nvalue :: Int\nvalue = \"wrong\"\n",
    )
    .unwrap();

    let mut diagnostics = harness.hooks.pause_next(Point::BeforeDiagnostics);
    harness.configure();
    loop {
        if matches!(
            harness.next().await,
            Event::ConfigurationFinished {
                outcome: iris_workspace::ConfigurationOutcome::Rebuilt,
                ..
            }
        ) {
            break;
        }
    }
    bounded(diagnostics.entered()).await;
    drop(diagnostics);
    let old = bounded(harness.events.recv()).await.unwrap();

    let mut preparation = harness.hooks.pause_next(Point::BeforePreparation);
    let sequence = harness.send(Command::Reload).unwrap();
    bounded(preparation.entered()).await;
    assert!(matches!(old.release(), Err(RequestFailure::Stale)));

    loop {
        if let Event::Diagnostics { diagnostics, .. } = harness.next().await {
            assert!(diagnostics.is_empty());
            break;
        }
    }

    drop(preparation);
    harness.ready(sequence).await;
    loop {
        if let Event::Diagnostics { diagnostics, .. } = harness.next().await {
            assert!(!diagnostics.is_empty());
            let mismatch = diagnostics.iter().any(|diagnostic| {
                diagnostic.message.contains("Int") && diagnostic.message.contains("String")
            });
            assert!(mismatch);
            break;
        }
    }
}

async fn diagnostics_for(harness: &mut Harness, uri: &Url) -> Vec<lsp_types::Diagnostic> {
    loop {
        let Event::Diagnostics { uri: published, diagnostics, .. } = harness.next().await else {
            continue;
        };
        if published == *uri {
            return diagnostics;
        }
    }
}

#[tokio::test]
async fn foreign_buffers_survive_rebuild_and_reopen_with_reset_versions() {
    let mut harness = Harness::new(Options::default());
    let source = "module Main where\nforeign import value :: Int\n";
    fs::write(harness.directory.path().join("Main.purs"), source).unwrap();
    let javascript = Url::from_file_path(harness.directory.path().join("Main.js")).unwrap();
    let jsx = Url::from_file_path(harness.directory.path().join("Main.jsx")).unwrap();

    let command = Document::Open {
        uri: Url::clone(&javascript),
        text: "export const value = 1;".into(),
        version: 8,
    };
    harness.send(Command::Document(command)).unwrap();
    harness.open(source, 1);
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let uri = Url::clone(&harness.uri);
    assert!(diagnostics_for(&mut harness, &uri).await.is_empty());

    let command = Document::Open {
        uri: Url::clone(&jsx),
        text: "export const value = 2;".into(),
        version: 9,
    };
    let sequence = harness.send(Command::Document(command)).unwrap();
    harness.ready(sequence).await;

    let ambiguous = diagnostics_for(&mut harness, &uri).await;
    assert!(!ambiguous.is_empty());

    let sequence = harness.send(Command::Reload).unwrap();
    harness.ready(sequence).await;

    let rebuilt = loop {
        let diagnostics = diagnostics_for(&mut harness, &uri).await;
        if !diagnostics.is_empty() {
            break diagnostics;
        }
    };
    assert_eq!(ambiguous, rebuilt);

    harness.send(Command::Document(Document::Close(javascript))).unwrap();
    harness.send(Command::Document(Document::Close(Url::clone(&jsx)))).unwrap();

    let command = Document::Open { uri: jsx, text: "export const value = 3;".into(), version: 1 };
    let sequence = harness.send(Command::Document(command)).unwrap();
    harness.ready(sequence).await;
    assert!(diagnostics_for(&mut harness, &uri).await.is_empty());
}

#[tokio::test]
async fn closing_a_buffer_only_source_clears_its_diagnostics() {
    let mut harness = Harness::new(Options::default());
    fs::remove_file(harness.directory.path().join("Main.purs")).unwrap();

    harness.open("module Main where\nvalue :: Int\nvalue = \"wrong\"\n", 1);
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let uri = Url::clone(&harness.uri);
    assert!(!diagnostics_for(&mut harness, &uri).await.is_empty());

    let sequence = harness.send(Command::Document(Document::Close(Url::clone(&uri)))).unwrap();
    harness.ready(sequence).await;
    assert!(diagnostics_for(&mut harness, &uri).await.is_empty());

    let (reply, request) = Reply::channel();
    harness.send(Command::LanguageServer(LanguageServer::DocumentSymbols { uri, reply })).unwrap();
    assert!(bounded(request).await.unwrap().release().unwrap().unwrap().is_none());
}

#[tokio::test]
async fn analysis_commands_return_locations_edits_and_stable_prim_uris() {
    let mut harness = Harness::new(Options {
        capabilities: iris_workspace::AnalyzerCapabilities::default().with_change_annotations(),
        ..Options::default()
    });
    harness.open(ORIGINAL, 6);
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let (reply, request) = Reply::channel();
    let command = LanguageServer::References {
        uri: Url::clone(&harness.uri),
        position: Position::new(5, 7),
        reply,
    };
    harness.send(Command::LanguageServer(command)).unwrap();

    let locations = bounded(request).await.unwrap().release().unwrap().unwrap().unwrap();
    let reference = locations
        .iter()
        .any(|location| location.uri == harness.uri && location.range.start.line == 5);
    assert!(reference);

    let (reply, request) = Reply::channel();
    let command = LanguageServer::Rename {
        uri: Url::clone(&harness.uri),
        position: Position::new(5, 7),
        new_name: "use".into(),
        reply,
    };
    harness.send(Command::LanguageServer(command)).unwrap();

    let edit = bounded(request).await.unwrap().release().unwrap().unwrap().unwrap();
    let changes = match edit.document_changes.unwrap() {
        lsp_types::DocumentChanges::Edits(edits) => edits,
        lsp_types::DocumentChanges::Operations(operations) => {
            let edits = operations.into_iter().filter_map(|operation| match operation {
                lsp_types::DocumentChangeOperation::Edit(edit) => Some(edit),
                _ => None,
            });

            edits.collect()
        }
    };

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].text_document.version, Some(6));
    assert_eq!(changes[0].edits.len(), 3);
    for edit in &changes[0].edits {
        let edit = match edit {
            lsp_types::OneOf::Left(edit) => edit,
            lsp_types::OneOf::Right(edit) => &edit.text_edit,
        };
        assert_eq!(edit.new_text, "use");
    }

    let (reply, request) = Reply::channel();
    let command = LanguageServer::SemanticTokens { uri: Url::clone(&harness.uri), reply };
    harness.send(Command::LanguageServer(command)).unwrap();

    let tokens = bounded(request).await.unwrap().release().unwrap().unwrap().unwrap();
    assert!(!tokens.data.is_empty());
    let legend = iris_workspace::semantic_tokens_legend();
    let keyword = &tokens.data[0];
    assert_eq!(keyword.length, 6);
    assert_eq!(
        legend.token_types[keyword.token_type as usize],
        lsp_types::SemanticTokenType::KEYWORD
    );
    assert_eq!(legend.token_modifiers, vec![lsp_types::SemanticTokenModifier::DECLARATION]);

    async fn prim(harness: &Harness) -> Url {
        let (reply, request) = Reply::channel();
        let command = LanguageServer::Definition {
            uri: Url::clone(&harness.uri),
            position: Position::new(2, 10),
            reply,
        };
        harness.send(Command::LanguageServer(command)).unwrap();

        match bounded(request).await.unwrap().release().unwrap().unwrap().unwrap() {
            lsp_types::GotoDefinitionResponse::Scalar(location) => location.uri,
            lsp_types::GotoDefinitionResponse::Array(locations) => Url::clone(&locations[0].uri),
            lsp_types::GotoDefinitionResponse::Link(locations) => {
                Url::clone(&locations[0].target_uri)
            }
        }
    }

    let first = prim(&harness).await;
    assert!(first.to_file_path().unwrap().is_file());

    let sequence = harness.send(Command::Reload).unwrap();
    harness.ready(sequence).await;
    assert_eq!(first, prim(&harness).await);
    assert!(first.to_file_path().unwrap().is_file());
}

#[tokio::test]
async fn discovery_preserves_root_and_literal_arguments_and_rejects_invalid_output() {
    let mut harness = Harness::new(Options::default());
    fs::write(harness.directory.path().join("Ignored.purs"), "module Ignored where\nignored = 0\n")
        .unwrap();
    fs::write(
        harness.directory.path().join("sources.cjs"),
        "require('fs').writeFileSync('arguments', process.argv[2]); console.log('Main.purs');",
    )
    .unwrap();

    let argument = "argument with spaces; $NOT_SHELL";
    let mut configuration = harness.configuration();
    configuration.settings.sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec!["sources.cjs".into(), argument.into()],
    };
    let sequence = harness.send(Command::Configure(configuration)).unwrap();
    harness.ready(sequence).await;

    assert_eq!(fs::read_to_string(harness.directory.path().join("arguments")).unwrap(), argument);

    let (reply, request) = Reply::channel();
    let command = LanguageServer::WorkspaceSymbols { query: "ignored".into(), reply };
    harness.send(Command::LanguageServer(command)).unwrap();

    let result = bounded(request).await.unwrap().release().unwrap().unwrap();
    assert!(match result {
        None => true,
        Some(lsp_types::WorkspaceSymbolResponse::Flat(symbols)) => symbols.is_empty(),
        Some(lsp_types::WorkspaceSymbolResponse::Nested(symbols)) => symbols.is_empty(),
    });

    let mut configuration = harness.configuration();
    configuration.settings.sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec!["-e".into(), "process.stdout.write(Buffer.from([255]))".into()],
    };
    harness.send(Command::Configure(configuration)).unwrap();

    loop {
        if let Event::StatusChanged(Status::Failed { message, .. }) = harness.next().await {
            assert!(message.contains("UTF-8"));
            break;
        }
    }
}

async fn process_tree(leader_exits: bool) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    let mut harness = Harness::new(Options::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let child = r#"
const address = { port: Number(process.argv[2]), host: '127.0.0.1' };
const socket = require('net').connect(address, () => {
    socket.write('ready\n');
    process.send('ready');
});

setInterval(() => {}, 1000);
setTimeout(() => process.exit(0), 30000);
"#;
    fs::write(harness.directory.path().join("child.cjs"), child).unwrap();

    let completion = if leader_exits {
        "console.log('Main.purs'); process.exit(0);"
    } else {
        "setInterval(() => {}, 1000);"
    };
    let parent = format!(
        r#"
const child = require('child_process').spawn(
    process.execPath,
    ['child.cjs', process.argv[2]],
    {{ stdio: ['ignore', 'inherit', 'inherit', 'ipc'] }},
);

child.on('message', () => {{ {completion} }});
setTimeout(() => process.exit(0), 30000);
"#
    );
    fs::write(harness.directory.path().join("sources.cjs"), parent).unwrap();

    let mut configuration = harness.configuration();
    configuration.settings.sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec!["sources.cjs".into(), port.to_string()],
    };
    let sequence = harness.send(Command::Configure(configuration)).unwrap();

    let (socket, _) = bounded(listener.accept()).await.unwrap();
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    bounded(reader.read_line(&mut line)).await.unwrap();
    assert_eq!(line, "ready\n");

    if leader_exits {
        harness.ready(sequence).await;
        assert!(harness.hover().await.contains("Int"));
    } else {
        harness.open(CHANGED, 1);
        harness.send(Command::Shutdown).unwrap();
        assert!(matches!(harness.status(), Status::Stopping | Status::Stopped));
        loop {
            if matches!(harness.next().await, Event::StatusChanged(Status::Stopped)) {
                break;
            }
        }
    }

    let mut remainder = vec![];
    assert_eq!(bounded(reader.read_to_end(&mut remainder)).await.unwrap(), 0);
}

#[tokio::test]
async fn shutdown_reaps_source_command_descendants() {
    process_tree(false).await;
}

#[tokio::test]
async fn successful_discovery_reaps_descendants_after_leader_exit() {
    process_tree(true).await;
}

#[tokio::test]
async fn authoritative_source_and_foreign_buffers_bypass_unreadable_disk_content() {
    let mut harness = Harness::new(Options::default());
    let source = "module Main where\nforeign import value :: Int\n";
    let foreign_path = harness.directory.path().join("Main.js");
    let foreign_uri = Url::from_file_path(&foreign_path).unwrap();
    fs::write(harness.directory.path().join("Main.purs"), [255]).unwrap();
    fs::write(&foreign_path, [255]).unwrap();

    harness.open(source, 3);
    let command =
        Document::Open { uri: foreign_uri, text: "export const value = 1;".into(), version: 5 };
    harness.send(Command::Document(command)).unwrap();
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let uri = Url::clone(&harness.uri);
    assert!(diagnostics_for(&mut harness, &uri).await.is_empty());
    assert!(harness.symbols().await.contains(&"value".into()));

    let sequence = harness.send(Command::Reload).unwrap();
    harness.ready(sequence).await;
    assert!(harness.symbols().await.contains(&"value".into()));
}

#[tokio::test]
async fn closing_an_excluded_source_does_not_restore_it_from_disk() {
    let mut harness = Harness::new(Options::default());
    harness.open(CHANGED, 1);
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let mut configuration = harness.configuration();
    configuration.settings.sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec!["-e".into(), "process.exit(0)".into()],
    };
    let sequence = harness.send(Command::Configure(configuration)).unwrap();
    harness.ready(sequence).await;
    assert!(harness.hover().await.contains("String"));

    harness.send(Command::Document(Document::Close(Url::clone(&harness.uri)))).unwrap();
    let sequence =
        harness.send(Command::Document(Document::Save(Url::clone(&harness.uri)))).unwrap();
    harness.ready(sequence).await;

    let (reply, request) = Reply::channel();
    let command = LanguageServer::DocumentSymbols { uri: Url::clone(&harness.uri), reply };
    harness.send(Command::LanguageServer(command)).unwrap();
    assert!(bounded(request).await.unwrap().release().unwrap().unwrap().is_none());
    assert!(harness.uri.to_file_path().unwrap().is_file());
}

#[tokio::test]
async fn failed_disk_reconciliation_clears_previously_published_diagnostics() {
    let mut harness = Harness::new(Options::default());
    harness.open("module Main where\nvalue :: Int\nvalue = \"wrong\"\n", 1);
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let uri = Url::clone(&harness.uri);
    assert!(!diagnostics_for(&mut harness, &uri).await.is_empty());

    fs::write(harness.directory.path().join("Main.purs"), [255]).unwrap();
    harness.send(Command::Document(Document::Close(Url::clone(&uri)))).unwrap();
    assert!(diagnostics_for(&mut harness, &uri).await.is_empty());

    loop {
        if matches!(harness.status(), Status::Failed { .. }) {
            break;
        }
        harness.next().await;
    }

    fs::write(harness.directory.path().join("Main.purs"), ORIGINAL).unwrap();
    let sequence = harness.send(Command::Reload).unwrap();
    harness.ready(sequence).await;
    assert!(harness.hover().await.contains("Int"));
}

#[tokio::test]
async fn request_admitted_behind_failure_is_rejected_without_another_input() {
    let mut harness = Harness::new(Options::default());
    harness.open(ORIGINAL, 1);
    let sequence = harness.configure();
    harness.ready(sequence).await;

    let mut failure = harness.hooks.pause_next(Point::BeforeFailure);
    fs::write(harness.directory.path().join("Main.purs"), [255]).unwrap();
    harness.send(Command::Document(Document::Close(Url::clone(&harness.uri)))).unwrap();
    bounded(failure.entered()).await;

    let request = harness.hover_request();
    drop(failure);

    assert!(matches!(bounded(request).await, Err(RequestFailure::Unavailable)));
    assert!(matches!(harness.status(), Status::Failed { .. }));
}

#[tokio::test]
async fn startup_waits_for_multiple_acknowledgements_and_finishes_once() {
    let mut harness = Harness::new(Options::default());
    let mut preparation = harness.hooks.pause_next(Point::BeforeAcknowledgement);
    harness.configure();
    bounded(preparation.entered()).await;

    harness.open(ORIGINAL, 1);
    let mut reconciliation = harness.hooks.pause_next(Point::BeforeAcknowledgement);
    drop(preparation);
    bounded(reconciliation.entered()).await;
    assert!(matches!(harness.status(), Status::Rebuilding { .. }));

    let sequence = harness.change(CHANGED, 2);
    drop(reconciliation);
    let mut finished = Vec::new();
    loop {
        match harness.next().await {
            Event::Finished { outcome, .. } => finished.push(outcome),
            Event::StatusChanged(Status::Ready { stamp, .. }) => {
                assert_eq!(stamp.revision, sequence);
                break;
            }
            _ => {}
        }
    }
    assert!(harness.hover().await.contains("String"));

    harness.send(Command::Shutdown).unwrap();
    while let Some(delivery) = bounded(harness.events.recv()).await {
        if let Ok(Event::Finished { outcome, .. }) = delivery.release() {
            finished.push(outcome);
        }
    }
    assert_eq!(finished, vec![Outcome::Ready]);
}

#[tokio::test]
async fn shutdown_waits_for_cancelled_worker_and_ignores_its_late_acknowledgement() {
    let mut harness = Harness::new(Options::default());
    let mut acknowledgement = harness.hooks.pause_next(Point::BeforeAcknowledgement);
    harness.configure();
    bounded(acknowledgement.entered()).await;

    harness.send(Command::Shutdown).unwrap();
    assert_eq!(harness.status(), Status::Stopping);
    assert!(matches!(harness.send(Command::Reload), Err(RequestFailure::Unavailable)));
    drop(acknowledgement);

    let mut finished = Vec::new();
    while let Some(delivery) = bounded(harness.events.recv()).await {
        let Ok(event) = delivery.release() else {
            continue;
        };
        assert!(!matches!(event, Event::StatusChanged(Status::Ready { .. })));
        if let Event::Finished { outcome, .. } = event {
            finished.push(outcome);
        }
    }
    assert_eq!(finished, vec![Outcome::Cancelled]);
    assert_eq!(harness.status(), Status::Stopped);
}

#[tokio::test]
async fn command_handles_and_cleanup_have_independent_ownership() {
    let WorkspaceSession { workspace, mut events, join } =
        Workspace::start(Options::default()).unwrap();
    let handle = Workspace::clone(&workspace);
    drop(workspace);

    handle.send(Command::Reload).unwrap();
    let held = bounded(events.recv()).await.unwrap();
    handle.send(Command::Shutdown).unwrap();
    drop(held);
    drop(events);

    bounded(tokio::task::spawn_blocking(move || join.join())).await.unwrap().unwrap();
    assert_eq!(handle.status(), Status::Stopped);
    assert!(matches!(handle.send(Command::Reload), Err(RequestFailure::Unavailable)));
}

#[tokio::test]
async fn progress_reports_a_phase_without_requiring_prior_events() {
    let mut harness = Harness::new(Options::default());
    let mut acknowledgement = harness.hooks.pause_next(Point::BeforeAcknowledgement);
    harness.configure();
    bounded(acknowledgement.entered()).await;

    loop {
        if let Event::Progress { phase: iris_workspace::Phase::Reconciling, .. } =
            harness.next().await
        {
            break;
        }
    }
    drop(acknowledgement);
}

async fn configuration_outcome(
    harness: &mut Harness,
    expected: InputSequence,
) -> iris_workspace::ConfigurationOutcome {
    loop {
        if let Event::ConfigurationFinished { sequence, outcome } = harness.next().await {
            assert_eq!(sequence, expected);
            return outcome;
        }
    }
}

#[tokio::test]
async fn configurations_classify_changes_and_retry_the_same_failed_settings() {
    use iris_workspace::ConfigurationOutcome;

    let mut harness = Harness::new(Options::default());
    let sequence = harness.configure();
    assert_eq!(configuration_outcome(&mut harness, sequence).await, ConfigurationOutcome::Rebuilt);
    let initial = harness.ready(sequence).await;

    let sequence = harness.configure();
    assert_eq!(
        configuration_outcome(&mut harness, sequence).await,
        ConfigurationOutcome::Unchanged
    );
    assert_eq!(harness.ready(sequence).await.incarnation, initial.incarnation);

    let mut policy = harness.configuration();
    policy.settings.diagnostics.on_change = true;
    let sequence = harness.send(Command::Configure(policy)).unwrap();
    assert_eq!(
        configuration_outcome(&mut harness, sequence).await,
        ConfigurationOutcome::PolicyUpdated
    );
    assert_eq!(harness.ready(sequence).await.incarnation, initial.incarnation);

    let source = "module Main where\nvalue :: Int\nvalue = \"wrong\"\n";
    let sequence = harness.open(source, 1);
    harness.ready(sequence).await;
    let uri = Url::clone(&harness.uri);
    assert!(!diagnostics_for(&mut harness, &uri).await.is_empty());

    let script = harness.directory.path().join("sources.cjs");
    fs::write(&script, "process.stderr.write('cannot discover'); process.exit(7);").unwrap();
    let mut configuration = harness.configuration();
    configuration.settings.sources =
        SourceDiscovery::Command { program: "node".into(), arguments: vec!["sources.cjs".into()] };
    let sequence =
        harness.send(Command::Configure(ConfigurationInput::clone(&configuration))).unwrap();
    let ConfigurationOutcome::Failed { message } =
        configuration_outcome(&mut harness, sequence).await
    else {
        panic!("discovery must fail");
    };
    assert!(message.contains("cannot discover"));

    fs::write(script, "console.log('Main.purs');").unwrap();
    let sequence = harness.send(Command::Configure(configuration)).unwrap();
    assert_eq!(configuration_outcome(&mut harness, sequence).await, ConfigurationOutcome::Rebuilt);
    assert_ne!(harness.ready(sequence).await.incarnation, initial.incarnation);
    assert!(!diagnostics_for(&mut harness, &uri).await.is_empty());

    harness.send(Command::Shutdown).unwrap();
    while let Some(delivery) = bounded(harness.events.recv()).await {
        assert!(!matches!(delivery.release(), Ok(Event::ConfigurationFinished { .. })));
    }
}

#[tokio::test]
async fn pending_configurations_each_finish_even_without_starting_preparation() {
    use iris_workspace::ConfigurationOutcome;

    let mut harness = Harness::new(Options::default());
    let initial = harness.configure();
    assert_eq!(configuration_outcome(&mut harness, initial).await, ConfigurationOutcome::Rebuilt);
    harness.ready(initial).await;

    let mut analysis = harness.hooks.pause_next(Point::BeforeAnalysis);
    let request = harness.hover_request();
    bounded(analysis.entered()).await;

    let mut configuration = harness.configuration();
    configuration.settings.sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec!["-e".into(), "console.log('Main.*')".into()],
    };
    let first = harness.send(Command::Configure(configuration)).unwrap();
    let second = harness.configure();
    harness.send(Command::Shutdown).unwrap();

    assert_eq!(configuration_outcome(&mut harness, first).await, ConfigurationOutcome::Superseded);
    assert_eq!(configuration_outcome(&mut harness, second).await, ConfigurationOutcome::Cancelled);
    drop(analysis);
    assert!(matches!(bounded(request).await, Err(RequestFailure::Cancelled)));

    while let Some(delivery) = bounded(harness.events.recv()).await {
        assert!(!matches!(delivery.release(), Ok(Event::ConfigurationFinished { .. })));
    }
}

#[tokio::test]
async fn running_preparation_reports_supersession_once() {
    use iris_workspace::ConfigurationOutcome;

    let mut harness = Harness::new(Options::default());
    let mut acknowledgement = harness.hooks.pause_next(Point::BeforeAcknowledgement);
    let first = harness.configure();
    bounded(acknowledgement.entered()).await;

    let second = harness.configure();
    assert_eq!(configuration_outcome(&mut harness, first).await, ConfigurationOutcome::Superseded);
    drop(acknowledgement);
    assert_eq!(configuration_outcome(&mut harness, second).await, ConfigurationOutcome::Rebuilt);

    harness.send(Command::Shutdown).unwrap();
    while let Some(delivery) = bounded(harness.events.recv()).await {
        assert!(!matches!(delivery.release(), Ok(Event::ConfigurationFinished { .. })));
    }
}

#[tokio::test]
async fn policy_updates_preserve_pending_diagnostics_and_control_future_edits() {
    let mut harness = Harness::new(Options::default());
    let mut diagnostics = harness.hooks.pause_next(Point::BeforeDiagnostics);
    let initial = harness.configure();
    configuration_outcome(&mut harness, initial).await;
    bounded(diagnostics.entered()).await;

    let mut policy = harness.configuration();
    policy.settings.diagnostics.on_open = false;
    policy.settings.diagnostics.on_change = true;
    let sequence = harness.send(Command::Configure(policy)).unwrap();
    assert_eq!(
        configuration_outcome(&mut harness, sequence).await,
        iris_workspace::ConfigurationOutcome::PolicyUpdated
    );
    drop(diagnostics);

    let uri = Url::clone(&harness.uri);
    assert!(diagnostics_for(&mut harness, &uri).await.is_empty());
    harness.open(ORIGINAL, 1);
    harness.change("module Main where\nvalue :: Int\nvalue = \"wrong\"\n", 2);
    assert!(!diagnostics_for(&mut harness, &uri).await.is_empty());
}

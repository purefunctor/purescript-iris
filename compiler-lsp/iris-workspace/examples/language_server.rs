//! Editor lifecycle, diagnostics, completion, and rename using one workspace.
//! Run with `cargo run -p iris-workspace --example language_server` (requires Node.js).

use std::time::Duration;

use configuration::{Configuration, SourceDiscovery};
use iris_workspace::{
    AnalyzerCapabilities, Command, ConfigurationInput, Document, Event, EventReceiver,
    InputSequence, LanguageServer, Options, Reply, RequestFailure, Status, Workspace,
};
use lsp_types::{
    CompletionItem, CompletionResponse, DiagnosticSeverity, DocumentChanges, HoverContents, OneOf,
    Position, PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent, Url,
};

const SOURCE: &str = "module Main where\n\nvalue :: Int\nvalue = 1\n\nuse = value\n";
const RENAMED: &str = "module Main where\n\ncount :: Int\ncount = 1\n\nuse = count\n";
const BUFFER: &str = "module Main where\nvalue = \"buffer\"\n";
const EDITED: &str = "module Main where\nvalue = true\n";
const VALID: &str = "module Main where\nvalue :: Int\nvalue = 42\n";
const BROKEN: &str = "module Main where\nvalue :: Int\nvalue = \"wrong\"\n";

fn main() {
    let directory = tempfile::tempdir().expect("create the example project");
    let path = directory.path().join("Main.purs");
    std::fs::write(&path, SOURCE).expect("write Main.purs");
    let uri = Url::from_file_path(path).expect("convert Main.purs to a file URI");

    let sources = SourceDiscovery::Command {
        program: "node".into(),
        arguments: vec!["-e".into(), "console.log('Main.purs')".into()],
    };
    let mut settings = Configuration { sources, ..Configuration::default() };
    settings.diagnostics.on_change = true;
    let configuration = ConfigurationInput { root: directory.path().to_path_buf(), settings };

    // The language server translates initialize capabilities into analyzer options. This client
    // can display change annotations and ask for confirmation before applying conflicting edits.
    let options = Options {
        capabilities: AnalyzerCapabilities::default().with_change_annotations(),
        ..Options::default()
    };
    let (workspace, mut events) = Workspace::start(options).expect("start the workspace service");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create the async runtime");
    let result = runtime.block_on(async {
        let scenarios = async {
            println!("Iris workspace: language-server walkthrough");
            lifecycle(&workspace, &mut events, configuration, &uri).await;
            diagnostics(&workspace, &mut events, &uri).await;
            completion_and_rename(&workspace, &mut events, &uri).await;
            shutdown(&workspace, &mut events).await;
        };
        tokio::time::timeout(Duration::from_secs(30), scenarios).await
    });

    // Joining waits for compiler snapshots and discovery descendants to retire. Do it outside
    // the async protocol loop, and before deleting the temporary project's files.
    workspace.join().expect("join the workspace controller without a panic");
    result.expect("language-server walkthrough timed out");
    println!("\nAll scenarios passed.");
}

async fn lifecycle(
    workspace: &Workspace,
    events: &mut EventReceiver,
    configuration: ConfigurationInput,
    uri: &Url,
) {
    println!("\n1. Open buffers, rebuilds, and cancellation");

    // Open buffers belong to the controller, not an engine incarnation. They can arrive even
    // before configuration and override disk when preparation eventually loads the project.
    let document = Document::Open { uri: Url::clone(uri), text: BUFFER.into(), version: 1 };
    workspace.send(Command::Document(document)).expect("admit the buffer before configuration");
    let sequence =
        workspace.send(Command::Configure(configuration)).expect("configure the project");
    ready(workspace, events, sequence).await;

    let (reply, request) = Reply::channel();
    let command =
        LanguageServer::Hover { uri: Url::clone(uri), position: Position::new(1, 1), reply };
    workspace.send(Command::LanguageServer(command)).expect("admit hover on the ready workspace");
    let held = request.await.expect("compute hover before the document changes");

    // A computed result is not yet safe to publish. Admitting another input revokes this held
    // delivery immediately, even if the worker has not applied that input yet.
    let change =
        TextDocumentContentChangeEvent { range: None, range_length: None, text: EDITED.into() };
    let document = Document::Change { uri: Url::clone(uri), version: 2, changes: vec![change] };
    let sequence = workspace.send(Command::Document(document)).expect("admit the version 2 edit");
    assert!(matches!(held.release(), Err(RequestFailure::Stale | RequestFailure::Cancelled)));
    println!("  Held hover rejected after didChange.");
    ready(workspace, events, sequence).await;

    let before = workspace.status();
    let sequence = workspace.send(Command::Reload).expect("admit the full rebuild");
    ready(workspace, events, sequence).await;
    assert_ne!(before, workspace.status());

    // Each variant fixes its reply type. Release at the publication boundary with no intervening
    // await or output queue; a multi-threaded adapter must serialize publication with inputs.
    let (reply, request) = Reply::channel();
    let command =
        LanguageServer::Hover { uri: Url::clone(uri), position: Position::new(1, 1), reply };
    workspace.send(Command::LanguageServer(command)).expect("admit hover after rebuilding");
    let delivery = request.await.expect("compute hover from the preserved buffer");
    let hover = delivery
        .release()
        .expect("hover must still match current inputs")
        .expect("the value declaration must have hover information");
    print_message("Hover after rebuild", serde_json::json!({"result": hover}));

    let expected = lsp_types::MarkedString::LanguageString(lsp_types::LanguageString {
        language: "purescript".into(),
        value: "value :: Boolean".into(),
    });
    assert_eq!(hover.contents, HoverContents::Array(vec![expected]));

    // Request cancellation is monotonic and nonblocking. It does not mean the worker has freed
    // its slot; future work still waits for the cancelled computation to actually finish.
    let (reply, request) = Reply::channel();
    request.cancellation().cancel();
    let command = LanguageServer::DocumentSymbols { uri: Url::clone(uri), reply };
    workspace.send(Command::LanguageServer(command)).expect("admit the already-cancelled request");
    assert!(matches!(request.await, Err(RequestFailure::Cancelled)));
    println!("  Cancelled request returned Cancelled.");

    let sequence = workspace
        .send(Command::Document(Document::Close(Url::clone(uri))))
        .expect("close the lifecycle buffer before the next scenario");
    ready(workspace, events, sequence).await;
}

async fn diagnostics(workspace: &Workspace, events: &mut EventReceiver, uri: &Url) {
    println!("\n2. Diagnostics through edit, save, and close");

    // textDocument/didOpen supplies the editor's text and version. The valid disk file must not
    // replace it: diagnostics should describe the unsaved error.
    let open = Document::Open { uri: Url::clone(uri), text: BROKEN.into(), version: 7 };
    workspace.send(Command::Document(open)).expect("accept didOpen");

    let initial = publish_next(events, uri, Some(7)).await;
    let mismatch = initial.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Some(DiagnosticSeverity::ERROR)
            && diagnostic.message.contains("Int")
            && diagnostic.message.contains("String")
    });
    assert!(mismatch, "the unsaved buffer must report its Int/String mismatch");

    // textDocument/didChange uses the position encoding negotiated during initialization.
    // Options::default selects UTF-16. This range replaces the quoted literal, including quotes.
    let change = TextDocumentContentChangeEvent {
        range: Some(Range::new(Position::new(2, 8), Position::new(2, 15))),
        range_length: None,
        text: "42".into(),
    };
    let document = Document::Change { uri: Url::clone(uri), version: 8, changes: vec![change] };
    workspace.send(Command::Document(document)).expect("accept the incremental didChange");

    let fixed = publish_next(events, uri, Some(8)).await;
    assert!(fixed.diagnostics.is_empty(), "the corrected buffer must clear the error");

    // didSave is a notification, not a command to write a file. The editor performs the write.
    // A watcher can then report that same write; the open document remains authoritative.
    let path = uri.to_file_path().expect("recover the example's source path");
    std::fs::write(path, VALID).expect("simulate the editor saving the corrected buffer");
    workspace.send(Command::Document(Document::Save(Url::clone(uri)))).expect("accept didSave");
    workspace.send(Command::FilesChanged(vec![Url::clone(uri)])).expect("accept the watcher event");

    let rebuilt = publish_next(events, uri, Some(8)).await;
    assert!(rebuilt.diagnostics.is_empty(), "the saved buffer must remain valid after rediscovery");

    // A buffer-only document has no disk source to restore on close. Publishing an empty list
    // is still necessary: silence would leave its last error visible in the editor.
    let scratch = uri.join("Scratch.purs").expect("construct a buffer-only file URI");
    let open = Document::Open {
        uri: Url::clone(&scratch),
        text: "module Scratch where\nvalue :: Int\nvalue = \"wrong\"\n".into(),
        version: 1,
    };
    workspace.send(Command::Document(open)).expect("accept didOpen for the buffer-only source");

    let broken = publish_next(events, &scratch, Some(1)).await;
    assert!(!broken.diagnostics.is_empty(), "the scratch buffer must have an error to clear");

    workspace
        .send(Command::Document(Document::Close(Url::clone(&scratch))))
        .expect("accept didClose");
    let closed = publish_next(events, &scratch, None).await;
    assert!(closed.diagnostics.is_empty(), "closing the scratch buffer must clear its diagnostics");

    let sequence = workspace
        .send(Command::Document(Document::Close(Url::clone(uri))))
        .expect("close the diagnostics buffer before the next scenario");
    ready(workspace, events, sequence).await;
}

async fn completion_and_rename(workspace: &Workspace, events: &mut EventReceiver, uri: &Url) {
    println!("\n3. Completion resolution and rename");

    let open = Document::Open { uri: Url::clone(uri), text: SOURCE.into(), version: 6 };
    let sequence = workspace.send(Command::Document(open)).expect("accept didOpen at version 6");
    ready(workspace, events, sequence).await;

    // The incoming textDocument/completion ID stays in the handler. The workspace receives only
    // semantic inputs and a reply channel whose result type is fixed by the command variant.
    let (reply, request) = Reply::channel();
    let command =
        LanguageServer::Completion { uri: Url::clone(uri), position: Position::new(5, 9), reply };
    workspace.send(Command::LanguageServer(command)).expect("admit the completion request");
    let delivery = request.await.expect("compute completion suggestions");
    let response = delivery.release().expect("completion must still match current inputs");
    print_message(
        "Completion",
        serde_json::json!({"jsonrpc": "2.0", "id": 101, "result": response}),
    );

    let items = match response.expect("the value prefix must have completions") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    // Keep the original item to resolve its outdated token again after the rename below.
    let item = items
        .into_iter()
        .find(|item| item.label == "value")
        .expect("completion must suggest the declared value");
    assert!(item.data.as_ref().is_some_and(serde_json::Value::is_string));

    // completionItem/resolve echoes the selected item's data unchanged. The string is an opaque
    // workspace token, not a file ID or something the adapter should decode or manufacture.
    let (reply, request) = Reply::channel();
    let command = LanguageServer::ResolveCompletion { item: CompletionItem::clone(&item), reply };
    workspace.send(Command::LanguageServer(command)).expect("admit completionItem/resolve");
    let delivery = request.await.expect("resolve the completion's type information");
    let resolved = delivery.release().expect("resolved completion must still be current");
    print_message(
        "Resolved completion",
        serde_json::json!({"jsonrpc": "2.0", "id": 102, "result": resolved}),
    );
    assert!(resolved.detail.as_ref().is_some_and(|detail| detail.contains("Int")));
    assert!(resolved.data.is_none(), "resolved items must not expose compiler identities");

    // Renaming value to use would change name resolution. With annotation support the result
    // asks the editor for confirmation, and records the open document version for its edits.
    let (reply, request) = Reply::channel();
    let command = LanguageServer::Rename {
        uri: Url::clone(uri),
        position: Position::new(5, 7),
        new_name: "use".into(),
        reply,
    };
    workspace.send(Command::LanguageServer(command)).expect("admit the conflicting rename");
    let delivery = request.await.expect("compute annotated rename edits");
    let edit = delivery
        .release()
        .expect("rename edits must still be current")
        .expect("the selected value must be renameable");
    print_message(
        "Rename requiring confirmation",
        serde_json::json!({"jsonrpc": "2.0", "id": 103, "result": edit}),
    );

    let annotations = edit.change_annotations.expect("conflicting edits need annotations");
    assert!(annotations.values().any(|annotation| annotation.needs_confirmation == Some(true)));
    let DocumentChanges::Edits(changes) =
        edit.document_changes.expect("rename must edit documents")
    else {
        panic!("this rename must contain text edits, not file operations");
    };
    assert_eq!(changes.len(), 1);
    assert_eq!(&changes[0].text_document.uri, uri);
    assert_eq!(changes[0].text_document.version, Some(6));
    assert_eq!(changes[0].edits.len(), 3);
    assert!(changes[0].edits.iter().any(|edit| matches!(edit, OneOf::Right(_))));

    // The editor declines that rename and requests a non-conflicting name instead. Returning
    // WorkspaceEdit does not modify the workspace: only subsequent didChange updates its text.
    let (reply, request) = Reply::channel();
    let command = LanguageServer::Rename {
        uri: Url::clone(uri),
        position: Position::new(5, 7),
        new_name: "count".into(),
        reply,
    };
    workspace.send(Command::LanguageServer(command)).expect("admit the non-conflicting rename");
    let delivery = request.await.expect("compute the count rename");
    let edit = delivery
        .release()
        .expect("count edits must still be current")
        .expect("value must have rename edits");
    print_message(
        "Rename to count",
        serde_json::json!({"jsonrpc": "2.0", "id": 104, "result": edit}),
    );

    let changes = edit.changes.expect("non-conflicting rename must return ordinary edits");
    let edits = changes.get(uri).expect("rename must edit Main.purs");
    for edit in edits {
        assert_eq!(edit.new_text, "count");
    }

    let mut ranges = edits.iter().map(|edit| edit.range).collect::<Vec<_>>();
    ranges.sort_by_key(|range| (range.start.line, range.start.character));
    assert_eq!(
        ranges,
        vec![
            Range::new(Position::new(2, 0), Position::new(2, 5)),
            Range::new(Position::new(3, 0), Position::new(3, 5)),
            Range::new(Position::new(5, 6), Position::new(5, 11)),
        ]
    );

    let change =
        TextDocumentContentChangeEvent { range: None, range_length: None, text: RENAMED.into() };
    let document = Document::Change { uri: Url::clone(uri), version: 7, changes: vec![change] };
    let sequence =
        workspace.send(Command::Document(document)).expect("accept the editor's applied rename");
    ready(workspace, events, sequence).await;

    // An editor may still hold a completion from before the rename. Resolving its old token
    // returns the item without stale type information, rather than looking up recycled IDs.
    let (reply, request) = Reply::channel();
    let command = LanguageServer::ResolveCompletion { item, reply };
    workspace
        .send(Command::LanguageServer(command))
        .expect("admit resolution of an outdated completion");
    let delivery = request.await.expect("handle the outdated completion token");
    let outdated = delivery.release().expect("token rejection must use current analysis");
    assert!(outdated.data.is_none());
    assert!(outdated.detail.is_none());
    println!("  Outdated completion token discarded after didChange.");
}

async fn shutdown(workspace: &Workspace, events: &mut EventReceiver) {
    println!("\n4. Shutdown");
    workspace.send(Command::Shutdown).expect("admit workspace shutdown");
    while let Some(delivery) = events.recv().await {
        if matches!(delivery.release(), Ok(Event::StatusChanged(Status::Stopped))) {
            break;
        }
    }
    assert_eq!(workspace.status(), Status::Stopped);
    println!("  Workspace stopped.");
}

async fn ready(workspace: &Workspace, events: &mut EventReceiver, sequence: InputSequence) {
    loop {
        match workspace.status() {
            Status::Ready { stamp, .. } if stamp.revision >= sequence => return,
            Status::Failed { message, .. } => panic!("preparation failed: {message}"),
            _ => {}
        }

        // These scripted scenarios drain events at milestones. A language server continuously
        // consumes the stream, including diagnostics and progress, in its output loop.
        let delivery = events.recv().await.expect("workspace stopped before becoming ready");
        if let Ok(Event::InputRejected { failure, .. }) = delivery.release() {
            panic!("invalid example input: {failure}");
        }
    }
}

async fn publish_next(
    events: &mut EventReceiver,
    expected_uri: &Url,
    expected_version: Option<i32>,
) -> PublishDiagnosticsParams {
    loop {
        let delivery = events.recv().await.expect("receive a workspace event before shutdown");

        // A language server performs this check in its output loop. Never release in a background
        // task and queue the unguarded value: a later edit could invalidate it before transmission.
        let Ok(event) = delivery.release() else {
            continue;
        };

        match event {
            Event::Diagnostics { uri, version, diagnostics } => {
                let publication = PublishDiagnosticsParams { uri, version, diagnostics };
                let name = publication
                    .uri
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .expect("diagnostics must refer to a source file");
                let version = publication
                    .version
                    .map_or("unversioned".into(), |version| format!("version {version}"));
                println!("  {name} ({version}) — diagnostics: {}", publication.diagnostics.len());
                for diagnostic in &publication.diagnostics {
                    println!("    {}", diagnostic.message);
                }

                // The output above summarizes textDocument/publishDiagnostics. Only the scripted
                // editor waits for a particular version; every valid publication is displayed.
                if publication.uri == *expected_uri && publication.version == expected_version {
                    return publication;
                }
            }
            Event::InputRejected { failure, .. } => panic!("editor sent invalid input: {failure}"),
            Event::StatusChanged(Status::Failed { message, .. }) => {
                panic!("preparation failed: {message}")
            }
            _ => {}
        }
    }
}

fn print_message(label: &str, message: serde_json::Value) {
    println!("\n  {label}");
    let message = serde_json::to_string_pretty(&message).expect("format the protocol message");
    for line in message.lines() {
        println!("    {line}");
    }
}

use std::path::Path;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use lsp_server::ErrorCode;
use lsp_types::ProgressToken;
use serde_json::{Value, json};
use url::Url;

#[path = "lsp/client.rs"]
mod client;
#[path = "lsp/spago.rs"]
mod spago;
#[path = "support.rs"]
mod support;

use client::{Error, LanguageServer};
use spago::SpagoControl;
use support::{TestWorkspace, normalize_process_status, replace_path_prefix, workspace_prefixes};

fn snapshot_json(name: &str, value: &Value) {
    let mut value = value.clone();
    normalize_process_statuses(&mut value);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!(name, serde_json::to_string_pretty(&value).unwrap());
    });
}

fn snapshot_workspace_json(name: &str, value: &Value, workspace: &Path) {
    let mut value = value.clone();
    let workspace_prefixes = workspace_prefixes(workspace);
    normalize_workspace_paths(&mut value, &workspace_prefixes);
    snapshot_json(name, &value);
}

fn normalize_workspace_paths(value: &mut Value, workspace_prefixes: &[String]) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_workspace_paths(value, workspace_prefixes);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                normalize_workspace_paths(value, workspace_prefixes);
            }
        }
        Value::String(string) => {
            for workspace_prefix in workspace_prefixes {
                *string = replace_path_prefix(string, workspace_prefix);
            }
            if string.contains("[WORKSPACE]") {
                *string = string.replace('\\', "/");
            }
        }
        Value::Bool(_) | Value::Null | Value::Number(_) => {}
    }
}

fn normalize_process_statuses(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(normalize_process_statuses),
        Value::Object(fields) => fields.values_mut().for_each(normalize_process_statuses),
        Value::String(string) => {
            *string = normalize_process_status(string);
        }
        Value::Bool(_) | Value::Null | Value::Number(_) => {}
    }
}

#[test]
fn snapshot_normalization_preserves_path_boundaries_and_platform_status_details() {
    let mut value = json!([
        r"C:\workspace\src\Main.purs",
        r"C:\workspace-other\src\Main.purs",
        "exit status: 7",
        "exit code: 9",
        "signal: 15",
    ]);
    normalize_workspace_paths(&mut value, &[r"C:\workspace".to_string()]);
    normalize_process_statuses(&mut value);

    let lines = value.as_array().unwrap().iter().map(|value| value.as_str().unwrap());
    insta::assert_snapshot!(lines.collect::<Vec<_>>().join("\n"), @r"
    [WORKSPACE]/src/Main.purs
    C:\workspace-other\src\Main.purs
    exit code: 7
    exit code: 9
    signal: 15
    ");
}

#[cfg(unix)]
#[test]
fn snapshot_normalization_includes_a_canonical_symlinked_temp_ancestor() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real workspace");
    let linked = directory.path().join("linked-workspace");
    fs::create_dir_all(real.join("project/src")).unwrap();
    symlink(&real, &linked).unwrap();
    let workspace = linked.join("project");
    let canonical = dunce::canonicalize(&workspace).unwrap();
    let mut value = json!([
        canonical.join("src/Main.purs").to_string_lossy(),
        Url::from_file_path(canonical.join("src/Main.purs")).unwrap(),
    ]);
    let prefixes = workspace_prefixes(&workspace);
    normalize_workspace_paths(&mut value, &prefixes);

    let lines = value.as_array().unwrap().iter().map(|value| value.as_str().unwrap());
    insta::assert_snapshot!(lines.collect::<Vec<_>>().join("\n"), @"
    [WORKSPACE]/src/Main.purs
    [WORKSPACE]/src/Main.purs
    ");
}

fn assert_diagnostic_triggers(
    server: &mut LanguageServer,
    root: &Path,
    on_open: bool,
    on_save: bool,
    on_change: bool,
) {
    assert_diagnostic_triggers_for(server, root, "Main.purs", on_open, on_save, on_change);
}

fn assert_diagnostic_triggers_for(
    server: &mut LanguageServer,
    root: &Path,
    file: &str,
    on_open: bool,
    on_save: bool,
    on_change: bool,
) {
    let uri = Url::from_file_path(root.join(file)).unwrap();
    let module = file.strip_suffix(".purs").unwrap();
    let text = format!("module {module} where\nvalue :: Int\nvalue = \"invalid\"\n");
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {"uri": uri, "languageId": "purescript", "version": 1, "text": text}
        }),
    );
    server.assert_diagnostics(&uri, 1, on_open);
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": format!("{text}\n")}]
        }),
    );
    server.assert_diagnostics(&uri, 2, on_change);
    server.notify("textDocument/didSave", json!({"textDocument": {"uri": uri}}));
    server.assert_diagnostics(&uri, 2, on_save);
}

#[test]
fn empty_configuration_preserves_spago_and_default_diagnostics() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromSpago = 42\n");

    let cases: &[&[&str]] = &[&["lsp"], &["lsp", "--stdio"]];
    for arguments in cases {
        let mut server = LanguageServer::start(&workspace, "", arguments, workspace.path());
        let symbols = server.request("workspace/symbol", json!({"query": "fromSpago"}));
        snapshot_workspace_json("empty_configuration_workspace_symbol", &symbols, workspace.path());
        assert_diagnostic_triggers(&mut server, workspace.path(), true, true, false);
        server.shutdown();
    }
}

#[test]
fn formats_unsaved_documents_with_utf16_ranges() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        "package:
  name: application
  dependencies: []
workspace: {}
",
    );
    let saved = "module Main where
value = 0
";
    workspace.write("src/Main.purs", saved);
    let mut server = LanguageServer::start(&workspace, "", &["lsp"], workspace.path());
    server.request("workspace/symbol", json!({"query": "value"}));
    let uri = Url::from_file_path(workspace.path().join("src/Main.purs")).unwrap();
    let source = "module Main where\r\n\r\nvalue=\"😀\"";
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {"uri": uri, "languageId": "purescript", "version": 1, "text": source}
        }),
    );
    let parameters =
        json!({"textDocument": {"uri": uri}, "options": {"tabSize": 2, "insertSpaces": true}});
    let edits = server
        .connection
        .request("textDocument/formatting", parameters.clone())
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(
        edits,
        json!([{
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 2, "character": 10}},
            "newText": r#"module Main where

value = "😀"
"#
        }])
    );
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": edits[0]["newText"]}]
        }),
    );
    let edits = server
        .connection
        .request("textDocument/formatting", parameters.clone())
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(edits, json!([]));
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 3},
            "contentChanges": [{"text": "module Main where

value=combine firstArgument secondArgument
"}]
        }),
    );
    let edits = server.connection.request("textDocument/formatting", json!({
        "textDocument": {"uri": uri}, "options": {"tabSize": 4, "insertSpaces": false, "lineWidth": 32}
    })).recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    assert_eq!(
        edits[0]["newText"],
        "module Main where

value = combine firstArgument
    secondArgument
"
    );
    let mut errors = Vec::new();
    for options in [
        json!({"tabSize": 0, "insertSpaces": true}),
        json!({"tabSize": 2, "insertSpaces": true, "lineWidth": 0}),
        json!({"tabSize": 2, "insertSpaces": true, "lineWidth": "40"}),
    ] {
        let result = server
            .connection
            .request(
                "textDocument/formatting",
                json!({
                    "textDocument": {"uri": uri}, "options": options
                }),
            )
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        let Err(Error::Response(error)) = result else {
            panic!("expected invalid formatting options to be rejected: {result:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidParams as i32);
        errors.push(json!({"options": options, "error": error}));
    }
    snapshot_json("formatting_invalid_options", &json!(errors));
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 4}, "contentChanges": [{"text": "module"}]
        }),
    );
    let edits = server
        .connection
        .request("textDocument/formatting", parameters)
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(edits, Value::Null);
    assert_eq!(workspace.read("src/Main.purs"), saved);
    server.shutdown();
}

#[test]
fn discovers_workspace_sources_when_opened_from_a_nested_package() {
    let workspace = TestWorkspace::empty();
    workspace.write("spago.yaml", "workspace: {}\n");
    workspace.write(
        "packages/application/spago.yaml",
        r#"package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "packages/application/src/Library.purs",
        "module Library where\nfromWorkspaceRoot = 42\n",
    );
    let package_root = workspace.path().join("packages/application");
    let mut server =
        LanguageServer::start(&workspace, "packages/application", &["lsp"], &package_root);

    let symbols = server.request("workspace/symbol", json!({"query": "fromWorkspaceRoot"}));

    snapshot_workspace_json("nested_package_workspace_symbol", &symbols, workspace.path());
    server.shutdown();
}

#[test]
fn prepares_the_spago_workspace_before_serving_analysis() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "project/spago.yaml",
        r#"package:
  name: application
  dependencies: []
workspace: {}
"#,
    );
    workspace.write(
        "project/src/Library.purs",
        r#"module Library where
fromFreshClone = 42
"#,
    );
    // The server starts outside the project; the editor's workspace folder names the project.
    let project = workspace.path().join("project");
    let mut server = LanguageServer::start(&workspace, "launcher", &["lsp"], &project);

    let symbols = server.request("workspace/symbol", json!({"query": "fromFreshClone"}));

    snapshot_workspace_json("prepared_workspace_symbol", &symbols, &project);
    workspace.assert_spago_calls("project", &[&["fetch", "-p", "application"]]);
    server.shutdown();
}

fn gated_workspace() -> (TestWorkspace, SpagoControl) {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "package:\n  name: application\n  dependencies: []\nworkspace: {}\n");
    workspace.write("src/Library.purs", "module Library where\nfromDisk = 0\n");
    let control = SpagoControl::start(&workspace);
    (workspace, control)
}

#[test]
fn cancels_workspace_preparation_for_the_active_progress_token() {
    let (workspace, control) = gated_workspace();
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT", "1");
    let mut server = LanguageServer::start_with_progress(&workspace);

    let begin = server.wait_for_progress("begin");
    let token: ProgressToken = serde_json::from_value(begin["params"]["token"].clone()).unwrap();
    let mut spago = control.wait_for_start("spago");
    let mut descendant = control.wait_for_start("descendant");

    for token in [ProgressToken::Int(99), ProgressToken::String("unknown".to_string())] {
        server.cancel_progress(token);
    }
    let request = server.request_async("workspace/symbol", json!({"query": "fromDisk"}));
    assert!(spago.is_running());
    assert!(descendant.is_running());

    server.cancel_progress(token.clone());
    server.cancel_progress(token);
    let end = server.wait_for_progress("end");
    snapshot_json(
        "cancelled_workspace_preparation_progress",
        &json!([begin["params"]["value"], end["params"]["value"]]),
    );
    let error = request
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for the cancelled preparation request")
        .expect_err("cancelled preparation unexpectedly answered its waiting request");
    let Error::Response(response) = error else {
        panic!("expected a response error after cancellation, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::RequestFailed as i32);
    snapshot_json("workspace_request_while_preparing", &serde_json::to_value(response).unwrap());

    spago.wait_for_exit();
    descendant.wait_for_exit();
    server.collect_notifications();
    assert!(!server.notifications.iter().any(|notification| {
        notification["method"] == "$/progress" && notification["params"]["value"]["kind"] == "end"
    }));
    server.shutdown();
}

#[cfg(unix)]
#[test]
fn cancellation_retires_a_pipe_holding_descendant_after_the_spago_leader_exits() {
    let (workspace, control) = gated_workspace();
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT", "1");
    workspace.set_env("IRIS_E2E_SPAGO_EXIT_AFTER_DESCENDANT", "1");
    let mut server = LanguageServer::start_with_progress(&workspace);

    let begin = server.wait_for_progress("begin");
    let mut spago = control.wait_for_start("spago");
    let mut descendant = control.wait_for_start("descendant");
    spago.wait_for_exit();
    assert!(descendant.is_running());

    server.cancel_progress(serde_json::from_value(begin["params"]["token"].clone()).unwrap());
    server.wait_for_progress("end");
    descendant.wait_for_exit();
    server.shutdown();
}

#[test]
fn analysis_request_restarts_cancelled_preparation_and_waits_for_the_workspace() {
    let (workspace, control) = gated_workspace();
    let mut server = LanguageServer::start_with_progress(&workspace);

    let first_begin = server.wait_for_progress("begin");
    let first_token: ProgressToken =
        serde_json::from_value(first_begin["params"]["token"].clone()).unwrap();
    let first_spago = control.wait_for_start("spago");

    let uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "purescript",
                "version": 1,
                "text": "module Library where\nfromBuffer = 1\n"
            }
        }),
    );
    server.cancel_progress(first_token);
    let first_end = server.wait_for_progress("end");

    let request_count = std::thread::available_parallelism().map_or(2, |count| count.get() + 1);
    let mut responses = (0..request_count)
        .map(|index| {
            let query = if index == 0 { "fromBuffer" } else { "noSuchSymbol" };
            server.request_async("workspace/symbol", json!({"query": query}))
        })
        .collect::<Vec<_>>();

    let second_begin = server.wait_for_progress("begin");
    assert_ne!(first_begin["params"]["token"], second_begin["params"]["token"]);
    let mut second_spago = control.wait_for_start("spago");
    assert!(!first_spago.is_running(), "first Spago fetch overlapped its retry");

    second_spago.release();
    let symbols = responses
        .remove(0)
        .recv_timeout(Duration::from_secs(30))
        .expect("timed out waiting for the original workspace request")
        .expect("original workspace request failed after retry");
    for response in responses {
        let symbols = response
            .recv_timeout(Duration::from_secs(30))
            .expect("timed out waiting for a coalesced workspace request")
            .expect("coalesced workspace request failed after retry");
        assert!(symbols.as_array().unwrap().is_empty());
    }
    assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "fromBuffer"));
    assert!(!symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "fromDisk"));
    snapshot_workspace_json("workspace_symbol_after_preparation_retry", &symbols, workspace.path());
    let second_end = server.wait_for_progress("end");
    snapshot_json(
        "workspace_preparation_retry_progress",
        &json!([
            first_begin["params"]["value"],
            first_end["params"]["value"],
            second_begin["params"]["value"],
            second_end["params"]["value"],
        ]),
    );
    workspace.assert_spago_calls(
        "",
        &[&["fetch", "-p", "application"], &["fetch", "-p", "application"]],
    );

    // One progress token per attempt, and the reports of the real preparation stay cancellable
    // with percentages of at most 100.
    let tokens = [&first_begin, &second_begin].map(|begin| begin["params"]["token"].clone());
    assert_eq!(first_end["params"]["token"], tokens[0]);
    assert_eq!(second_end["params"]["token"], tokens[1]);
    let created = server.client.progress_tokens.lock().unwrap().clone();
    let created = created.iter().map(|token| serde_json::to_value(token).unwrap());
    assert_eq!(created.collect::<Vec<_>>(), tokens);
    server.collect_notifications();
    let progress =
        server.notifications.iter().filter(|notification| notification["method"] == "$/progress");
    for notification in progress {
        assert!(tokens.contains(&notification["params"]["token"]), "{notification}");
        let value = &notification["params"]["value"];
        if value["kind"] == "report" {
            assert_eq!(value["cancellable"], true);
            assert!(value["percentage"].as_u64().unwrap() <= 100);
        }
    }

    // Closing the buffer restores the discovered source from disk.
    server.notify("textDocument/didClose", json!({"textDocument": {"uri": uri}}));
    server.wait_for_symbol("fromDisk", true);
    server.wait_for_symbol("fromBuffer", false);
    server.shutdown();
}

#[test]
fn shutdown_retires_the_spago_process_tree_while_preparing() {
    let (workspace, control) = gated_workspace();
    workspace.set_env("IRIS_E2E_SPAGO_DESCENDANT", "1");
    let mut server = LanguageServer::start_with_progress(&workspace);
    server.wait_for_progress("begin");
    let spago = control.wait_for_start("spago");
    let descendant = control.wait_for_start("descendant");
    let uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
    let document_symbols = server.document_symbols_async(uri);
    assert!(matches!(
        document_symbols.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));

    server.request_shutdown();
    let error = document_symbols
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for document symbols to terminate during shutdown")
        .expect_err("document symbols unexpectedly succeeded during shutdown");
    let Error::Response(response) = error else {
        panic!("expected a response error during shutdown, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::ContentModified as i32);
    let end = server.wait_for_progress("end");
    snapshot_json("shutdown_progress", &end["params"]["value"]);

    server.finish_shutdown();
    assert!(!spago.is_running(), "Spago survived shutdown");
    assert!(!descendant.is_running(), "Spago descendant survived shutdown");
}

#[test]
fn preparation_failure_is_reported_and_rejects_analysis() {
    let (workspace, control) = gated_workspace();
    workspace.set_env("IRIS_E2E_SPAGO_FAIL", "simulated spago failure\n");

    let mut server = LanguageServer::start_with_progress(&workspace);
    let mut spago = control.wait_for_start("spago");
    let uri = Url::from_file_path(workspace.path().join("src/Library.purs")).unwrap();
    let document_symbols = server.document_symbols_async(uri);
    assert!(matches!(
        document_symbols.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    ));
    spago.release();

    let message = server.wait_for_notification("window/showMessage");
    let end = server.wait_for_progress("end");
    snapshot_workspace_json(
        "workspace_preparation_failure",
        &json!({
            "message": message["params"],
            "progress": end["params"]["value"],
        }),
        workspace.path(),
    );
    let error = document_symbols
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for deferred request after preparation failure")
        .expect_err("deferred request unexpectedly succeeded after preparation failure");
    let Error::Response(response) = error else {
        panic!("expected a response error after failure, got {error:?}");
    };
    assert_eq!(response.code, ErrorCode::RequestFailed as i32);

    let error = server
        .request_once("workspace/symbol", json!({"query": "fromDisk"}))
        .expect_err("invariant violated: failed workspace answered a request");
    let Error::Response(response) = error else {
        panic!("expected a response error after failure, got {error:?}");
    };
    snapshot_json(
        "workspace_request_after_preparation_failure",
        &serde_json::to_value(response).unwrap(),
    );
    server.shutdown();
}

#[cfg(unix)]
#[test]
fn discovers_workspace_sources_through_a_symlinked_root() {
    use std::os::unix::fs::symlink;

    let workspace = TestWorkspace::empty();
    workspace.write(
        "project/spago.yaml",
        "package:\n  name: application\n  dependencies: []\nworkspace: {}\n",
    );
    workspace.write("project/src/Library.purs", "module Library where\nfromSymlinkRoot = 42\n");
    symlink(workspace.path().join("project"), workspace.path().join("linked-project")).unwrap();
    let linked_root = workspace.path().join("linked-project");
    let mut server = LanguageServer::start(&workspace, "linked-project", &["lsp"], &linked_root);

    let symbols = server.request("workspace/symbol", json!({"query": "fromSymlinkRoot"}));

    snapshot_workspace_json("symlinked_workspace_symbol", &symbols, workspace.path());
    server.shutdown();
}

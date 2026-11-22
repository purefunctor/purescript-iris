use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::support::{TestWorkspace, assert_success};
use super::watch::WatchProcess;

struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
    reader: Option<JoinHandle<()>>,
    next_id: u64,
}

impl McpClient {
    fn start(workspace: &TestWorkspace, arguments: &[&str]) -> McpClient {
        let mut child = workspace
            .command_builder("", arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("invariant violated: MCP stdout is not piped");
        let (sender, messages) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                let message = serde_json::from_str::<Value>(&line);
                let message = message.unwrap_or_else(|error| {
                    panic!("the MCP server wrote a line that is not JSON ({error}): {line}")
                });
                if sender.send(message).is_err() {
                    return;
                }
            }
        });
        McpClient { child, stdin, messages, reader: Some(reader), next_id: 1 }
    }

    fn initialize(&mut self) -> Value {
        let parameters = json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "tests-e2e", "version": "0.0.0" },
        });
        let response = self.request("initialize", parameters);
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        response
    }

    fn request(&mut self, method: &str, parameters: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": parameters }));
        loop {
            let message = self.messages.recv_timeout(Duration::from_secs(30));
            let message =
                message.unwrap_or_else(|error| panic!("no response to {method}: {error}"));
            if message["id"] == id {
                return message;
            }
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({ "name": name, "arguments": arguments }));
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("invariant violated: MCP stdin is closed");
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn close(mut self) -> std::process::ExitStatus {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "the MCP server kept running after stdin closed");
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if self.child.try_wait().unwrap().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn shapes_workspace() -> TestWorkspace {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Shapes.purs",
        r#"module Shapes where

-- | A shape with a size.
data Shape = Square Int | Circle Int

-- | The size of a shape.
size :: Shape -> Int
size (Square side) = side
size (Circle radius) = radius

class Area a where
  area :: a -> Int

instance Area Shape where
  area shape = size shape
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Shapes (Shape(..), size)

main :: Int
main = size (Square 2)
"#,
    );
    workspace
}

fn assert_matches_cli(
    workspace: &TestWorkspace,
    client: &mut McpClient,
    tool: &str,
    arguments: Value,
    query: &str,
) {
    let result = client.call(tool, arguments);
    assert_eq!(result["isError"], false, "{tool}: {result}");

    let mut command = vec!["watch", "query", "--json"];
    command.extend(query.split(' '));
    let json = workspace.command(&command);
    assert_success(&json);
    let response = serde_json::from_slice::<Value>(&json.stdout).unwrap();
    assert_eq!(result["structuredContent"], response["value"], "{tool}");

    let mut command = vec!["watch", "query"];
    command.extend(query.split(' '));
    let text = workspace.command(&command);
    assert_success(&text);
    let text = String::from_utf8_lossy(&text.stdout);
    let text = text.strip_suffix('\n').unwrap();
    assert_eq!(result["content"][0]["text"], text, "{tool}");
}

fn tool_summary(tool: &Value) -> String {
    let schema = &tool["inputSchema"];
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    let parameters = schema["properties"].as_object().unwrap().keys().map(|parameter| {
        if required.contains(&Value::from(parameter.as_str())) {
            parameter.clone()
        } else {
            format!("{parameter}?")
        }
    });
    let parameters = parameters.collect::<Vec<_>>().join(", ");
    let hints = ["readOnlyHint", "destructiveHint", "idempotentHint", "openWorldHint"];
    let hints = hints.iter().filter(|hint| tool["annotations"][**hint] == true);
    let hints = hints.copied().collect::<Vec<_>>().join(" ");
    format!("{}({parameters}) {hints}", tool["name"].as_str().unwrap())
}

#[test]
fn answers_watch_queries_through_mcp_tools() {
    let workspace = shapes_workspace();
    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));

    let mut client = McpClient::start(&workspace, &["watch", "mcp"]);
    let initialize = client.initialize();
    assert_eq!(initialize["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(initialize["result"]["serverInfo"]["name"], "iris");
    let instructions = initialize["result"]["instructions"].as_str().unwrap();
    assert!(!instructions.is_empty());

    let tools = client.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    for tool in tools {
        assert_eq!(tool["outputSchema"]["type"], "object", "{}", tool["name"]);
    }
    let summary = tools.iter().map(tool_summary).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(summary, @"
    definition(name, namespace?) readOnlyHint
    dependents(module) readOnlyHint
    diagnostics(module?) readOnlyHint
    instances(name, search) readOnlyHint
    javascript(module) readOnlyHint
    module(module) readOnlyHint
    refactor_let_signatures(module?, preview?) idempotentHint
    references(name, namespace?) readOnlyHint
    search(pattern) readOnlyHint
    signature(name, namespace?) readOnlyHint
    wait() idempotentHint
    ");

    for (tool, arguments, query) in [
        ("signature", json!({ "name": "Shapes.size" }), "signature Shapes.size"),
        ("module", json!({ "module": "Shapes" }), "module Shapes"),
        (
            "definition",
            json!({ "name": "Shapes.Shape", "namespace": "type" }),
            "definition type Shapes.Shape",
        ),
        ("references", json!({ "name": "Shapes.size" }), "references Shapes.size"),
        ("dependents", json!({ "module": "Shapes" }), "dependents Shapes"),
        (
            "instances",
            json!({ "name": "Shapes.Area", "search": "class" }),
            "instances class Shapes.Area",
        ),
        ("search", json!({ "pattern": "siz" }), "search siz"),
        ("diagnostics", json!({}), "diagnostics"),
        ("javascript", json!({ "module": "Main" }), "javascript Main"),
    ] {
        assert_matches_cli(&workspace, &mut client, tool, arguments, query);
    }

    let definition = client.call("definition", json!({ "name": "Shapes.size" }));
    insta::assert_snapshot!(serde_json::to_string_pretty(&definition).unwrap(), @r#"
    {
      "content": [
        {
          "text": "src/Shapes.purs:7:1",
          "type": "text"
        }
      ],
      "isError": false,
      "structuredContent": {
        "locations": [
          {
            "column": 1,
            "line": 7,
            "path": "src/Shapes.purs"
          }
        ]
      }
    }
    "#);

    let missing = client.call("signature", json!({ "name": "Shapes.missing" }));
    insta::assert_snapshot!(serde_json::to_string_pretty(&missing).unwrap(), @r#"
    {
      "content": [
        {
          "text": "module Shapes has no item named `missing`",
          "type": "text"
        }
      ],
      "isError": true
    }
    "#);

    workspace.write(
        "src/Main.purs",
        r#"module Main where

main :: Int
main = "oops"
"#,
    );
    let wait = client.call("wait", json!({}));
    assert_eq!(wait["structuredContent"], json!({ "build": { "outcome": "diagnostics" } }));
    let diagnostics = client.call("diagnostics", json!({ "module": "Main" }));
    insta::assert_snapshot!(
        serde_json::to_string_pretty(&diagnostics["structuredContent"]).unwrap(),
        @r#"
    {
      "diagnostics": [
        {
          "code": "CannotUnify",
          "location": {
            "column": 8,
            "line": 4,
            "path": "src/Main.purs"
          },
          "message": "Could not match 'String' with 'Int'",
          "severity": "error"
        }
      ]
    }
    "#
    );
    assert_matches_cli(&workspace, &mut client, "diagnostics", json!({}), "diagnostics");
}

#[test]
fn finds_a_watcher_that_starts_after_the_mcp_server() {
    let workspace = shapes_workspace();
    let mut client = McpClient::start(&workspace, &["watch", "mcp"]);
    client.initialize();

    let missing = client.call("wait", json!({}));
    assert_eq!(missing["isError"], true);
    let message = missing["content"][0]["text"].as_str().unwrap();
    insta::assert_snapshot!(workspace.normalize_output(message.as_bytes()), @"no `iris watch` is running for [WORKSPACE]/output; start one with `iris watch`, or retry if it is starting");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));
    let wait = client.call("wait", json!({}));
    assert_eq!(wait["isError"], false, "{wait}");
    assert_eq!(wait["structuredContent"], json!({ "build": { "outcome": "succeeded" } }));
    assert_eq!(wait["content"][0]["text"], "Build succeeded.");

    assert!(client.close().success());
}

#[test]
fn answers_stateless_requests_without_a_handshake() {
    let workspace = shapes_workspace();
    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));

    let mut client = McpClient::start(&workspace, &["watch", "mcp"]);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    let discover = client.request("server/discover", json!({ "_meta": meta }));
    let versions = discover["result"]["supportedVersions"].as_array();
    assert!(versions.is_some_and(|versions| versions.contains(&json!("2026-07-28"))), "{discover}");
    let call =
        json!({ "name": "signature", "arguments": { "name": "Shapes.size" }, "_meta": meta });
    let signature = client.request("tools/call", call);
    assert_eq!(signature["result"]["isError"], false, "{signature}");
    assert_eq!(signature["result"]["resultType"], "complete");
    assert_eq!(
        signature["result"]["structuredContent"]["declarations"][0]["signature"],
        "size :: Shape -> Int"
    );
}

#[test]
fn refactors_let_signatures_through_mcp() {
    let workspace = shapes_workspace();
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Shapes (Shape(..), size)

main :: Int
main =
  let
    shape = Square 2
  in
    size shape
"#,
    );
    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));
    let mut client = McpClient::start(&workspace, &["watch", "mcp"]);
    client.initialize();

    let before = workspace.read("src/Main.purs");
    let cli_preview =
        workspace.command(&["watch", "refactor", "--preview", "let-signatures", "Main"]);
    assert_success(&cli_preview);
    let preview =
        client.call("refactor_let_signatures", json!({ "module": "Main", "preview": true }));
    assert_eq!(preview["isError"], false, "{preview}");
    assert_eq!(preview["structuredContent"]["written"], false);
    let patch = String::from_utf8_lossy(&cli_preview.stdout);
    assert_eq!(preview["structuredContent"]["patch"], *patch);
    assert_eq!(preview["content"][0]["text"], *patch);
    assert_eq!(workspace.read("src/Main.purs"), before);

    let written = client.call("refactor_let_signatures", json!({ "module": "Main" }));
    assert_eq!(written["isError"], false, "{written}");
    assert_eq!(written["structuredContent"]["written"], true);
    assert!(written["structuredContent"].get("patch").is_none(), "{written}");
    insta::assert_snapshot!(written["content"][0]["text"].as_str().unwrap(), @"Added 1 signature across 1 file.");
    insta::assert_snapshot!(workspace.read("src/Main.purs"), @"
    module Main where

    import Shapes (Shape(..), size)

    main :: Int
    main =
      let
        shape :: Shape
        shape = Square 2
      in
        size shape
    ");

    let wait = client.call("wait", json!({}));
    assert_eq!(wait["structuredContent"], json!({ "build": { "outcome": "succeeded" } }));

    workspace.write(
        "src/Broken.purs",
        r#"module Broken where

broken :: Int
broken =
  let
    text = "oops"
  in
    text
"#,
    );
    let wait = client.call("wait", json!({}));
    assert_eq!(wait["structuredContent"], json!({ "build": { "outcome": "diagnostics" } }));
    let broken_before = workspace.read("src/Broken.purs");
    let broken = client.call("refactor_let_signatures", json!({ "module": "Broken" }));
    assert_eq!(broken["isError"], true, "{broken}");
    assert!(broken.get("structuredContent").is_none(), "{broken}");
    insta::assert_snapshot!(broken["content"][0]["text"].as_str().unwrap(), @"
    No changes.
    Skipped 1:
    src/Broken.purs:1:1: Broken (module_has_errors: 1 checking error)
    ");
    assert_eq!(workspace.read("src/Broken.purs"), broken_before);
}

#[test]
fn finds_a_watcher_with_a_custom_output_directory() {
    let workspace = shapes_workspace();
    let mut watch = WatchProcess::new(workspace.spawn(&["watch", "--output", "generated"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));

    for arguments in [
        &["watch", "--output", "generated", "mcp"][..],
        &["watch", "mcp", "--output", "generated"][..],
    ] {
        let mut client = McpClient::start(&workspace, arguments);
        client.initialize();
        let wait = client.call("wait", json!({}));
        assert_eq!(wait["isError"], false, "{arguments:?}: {wait}");
        assert_eq!(wait["structuredContent"], json!({ "build": { "outcome": "succeeded" } }));
    }

    let mut client = McpClient::start(&workspace, &["watch", "mcp"]);
    client.initialize();
    let missing = client.call("wait", json!({}));
    assert_eq!(missing["isError"], true, "{missing}");
    let message = missing["content"][0]["text"].as_str().unwrap();
    insta::assert_snapshot!(workspace.normalize_output(message.as_bytes()), @"no `iris watch` is running for [WORKSPACE]/output; start one with `iris watch`, or retry if it is starting");
}

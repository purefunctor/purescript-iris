# iris-lsp

`iris-lsp` runs the Iris language server. `iris-cli` calls `iris_lsp::start` for `iris lsp`.
`start` builds a Tokio runtime, creates two actors, and connects them:

- **`iris-lsp-server`** (`iris_lsp_server::Server`) is the only component that talks to the
  editor. It owns the stdio transport (through `lsp-server`), the LSP lifecycle, the IDs of
  requests received from the editor and `$/cancelRequest`, every message sent to the editor,
  monitoring of the editor process, and the order of steps when the server stops. It depends on
  no Iris crates.
- **`iris-lsp-workspace`** (`iris_lsp_workspace::WorkspaceService`) owns Iris state: workspace
  preparation (`spago fetch` and the initial build), settings, open documents, the query engine
  and its snapshots, analysis requests, and diagnostics. It never addresses the editor.

```text
Editor <-> lsp-server stdio threads <-> iris-lsp-server --OrderedMessage---> iris-lsp-workspace
                                                        --ControlMessage--->
                                                        <--WorkspaceEvent---
                                                        <--replies----------
```

Each actor can be tested without the other: `iris-lsp-server`'s tests play the editor and the
workspace actor, and `iris-lsp-workspace`'s tests exchange messages with the actor directly.
`iris-lsp`'s own tests run both actors over an in-memory connection.

## Messages between the actors

The message types live in `iris-lsp-server` (`service.rs`):

- `OrderedMessage`: `initialize`, `initialized`, settings, document notifications, and analysis
  requests. Parameters and results of analysis requests pass through `iris-lsp-server` as
  JSON keyed by LSP method name; the workspace actor decodes them.
- `ControlMessage`: cancelling a preparation attempt from its progress token, and `shutdown`.
  They travel on their own channel, so they never wait behind ordered messages.
- `WorkspaceEvent`: what happened, in the workspace actor's terms: diagnostics for a file,
  preparation stages, and errors. `iris-lsp-server` turns them into `textDocument/publishDiagnostics`,
  `$/progress`, and `window/showMessage`.

Each request carries a reply channel. The workspace actor answers with a JSON result or a
`Rejection`, which `iris-lsp-server` maps to a JSON-RPC error code. Cancelling a request means
dropping its reply channel: on `$/cancelRequest`, `iris-lsp-server` drops the receiving end and
answers `RequestCancelled`; the workspace actor notices the closed channel and drops the queued
work and its snapshot.

All channels between the actors are unbounded. Only running work is limited, by one semaphore
for analysis requests and one for diagnostic collection.

## Ordering

The workspace actor handles `OrderedMessage`s one at a time, in the order they were sent. A
request sees every notification sent before it and none sent after it: it takes its snapshot
when the actor reaches it, after the actor applied every earlier notification. While
preparation runs, document notifications are buffered and replayed in order before a waiting
request takes its snapshot, so that request does not see notifications sent after it.

## Edits win over analysis

Analysis requests and diagnostic collection read a snapshot and run in parallel. A notification
that changes the query engine's inputs (`didOpen`, `didChange`, `didClose`,
`didChangeWatchedFiles`, and replaying buffered notifications) is applied as soon as the
workspace actor reaches it:

1. every analysis or diagnostic task still waiting for a permit drops its snapshot;
2. the change runs on a blocking thread and calls `QueryEngine::request_cancel`, whose cancelled
   flag stops running analysis at its next query;
3. the change waits only until running analysis notices the flag.

Analysis cancelled this way is answered with `ContentModified` (`"Content modified"`). A
cancelled diagnostic task publishes nothing; the change schedules new collections. Settings
changes and `didSave` do not change engine inputs, so they cancel nothing. A cancelled request
is not restarted, because its positions refer to the document it was sent for; the editor
decides whether to send it again.

## Answers may reflect earlier settings

A request is answered from the snapshot it took. Settings applied afterwards do not re-run,
delay, or invalidate it, so a request that waited for preparation may reflect the settings from
before a later change. Answers are not tagged with settings generations. `workspace/configuration`
responses still carry a generation number, so an older response cannot replace newer settings.

## Stopping

`shutdown` sends `ControlMessage::Shutdown` and is answered immediately: preparation is
cancelled, requests waiting for it are answered `ContentModified` (`"Workspace is loading"`), and
no diagnostic task starts afterwards. Cleanup starts when the connection ends (`exit`, end of
input, the editor process exiting, or a fatal error) and is limited to five seconds. It kills and
reaps Spago process trees, waits for blocking preparation work and for analysis and diagnostic
workers, and joins the stdio threads. Reads from standard input cannot be interrupted, so the
reading threads are joined only once the editor's input ended; otherwise only queued messages
are flushed. If cleanup takes longer than five seconds, `iris lsp` logs the failure and exits
with an error. `exit` without `shutdown` exits with status 1.

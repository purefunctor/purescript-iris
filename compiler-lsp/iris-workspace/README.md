# Iris workspace service

`iris-workspace` manages editor workspace analysis independently of `iris-lsp`. It uses
`iris-build` to find and load project files, then runs the analyzer to answer requests such as
hover and completion. Communication with the editor, matching responses to requests, agreeing
on supported features, and displaying progress are handled outside this crate.

Text from open editor documents takes precedence over files on disk. The service accepts and
orders input changes without waiting for compiler work to finish. A separate worker runs
compilation and analysis. Cancelled work may still be running while a rebuild waits to start.

Rebuilds keep open documents but make analysis unavailable until the latest inputs have been
loaded. If preparation fails, analysis remains unavailable rather than falling back to the
previous compiler state. Type errors in the source do not prevent the service from answering
analysis requests once those inputs are loaded.

An analysis result can become outdated while waiting to be sent to the editor. The caller must
check it when sending, including when the result reports an error. Diagnostics need the same
check. The service provides `Delivery::release` for this purpose; checking before putting a result
in another queue is too early. Its API documentation describes how the caller must coordinate
sending results with receiving new input.

The service keeps the latest queued status, progress, and diagnostics rather than accumulating
every update. Callers should finish handling each event before requesting the next, and allow
event handling to stop when the connection closes.

## Runnable walkthrough

The [language-server example](examples/language_server.rs) uses one temporary project and workspace,
with separate functions demonstrating:

- Lifecycle: configuration, open buffers, hover, rejection of outdated
  results, rebuilds, cancellation, and shutdown.
- Diagnostics: an unsaved error, an incremental edit, saving and file
  watcher notifications, and clearing diagnostics when a buffer-only document closes.
- Completion and rename: editor-owned request IDs, completion
  resolution, conflicting rename confirmation, versioned edits, and outdated completion tokens.

It explains the caller's responsibilities alongside executable requests and assertions, prints
labelled results, and removes its temporary project on exit. Node.js is required.

```sh
cargo run -p iris-workspace --example language_server
```

Tests in `tests/` run sequences of workspace commands against the real compiler. They pause work
at predefined points to control the order of operations.
Run them with `cargo nextest run -p iris-workspace`.

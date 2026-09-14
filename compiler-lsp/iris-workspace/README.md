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

Computed successes and failures remain inside `Delivery` until `Delivery::release` checks their
validity. Diagnostics use the same check. Admission, channel, and cancellation failures can be
returned before a delivery exists; computed failures must only be unwrapped after release.

Release belongs at irrevocable, ordered commitment to a reserved slot in the transport's final
writer, not physical socket flush. The adapter must serialize this commitment with input admission.
Releasing before a forwarding queue or router serialization is insufficient. Stock async-lsp
0.2.4 has no public deferred-output reservation API; migration requires transport support for this
boundary. The runnable example illustrates workspace semantics, not that transport integration.

An event pump may hold one delivery while awaiting writer commitment or discard, then receive the
next. It must not eagerly release events into an unbounded transport queue. Teardown must drop or
acknowledge the held delivery so that the pump can exit.

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

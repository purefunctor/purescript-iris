<h1 align="center">iris</h1>
<p align="center">a language implementation for PureScript</p>

---

Iris is a language implementation for PureScript, powered by an incremental, query-based build
system. Instead of a sequence of compiler phases, Iris models compilation and semantic information
as incrementally computed queries. These queries are used extensively to implement code intelligence
features in the language server.

The build system is designed with interactive editing in mind. To support this, it tracks dependencies
between inputs and queries, caches query results, deduplicates in-progress work across threads, and
supports cooperative cancellation when inputs change. Crucially, many query results are designed to
be incrementally reusable. For example, the compiler uses stable identities in lieu of source ranges 
to enable minimal recomputation across trivial formatting changes.

The language server component implements core code intelligence features such as completion, jump to
definition, hover information, find references, workspace symbol search, and diagnostics.

## Language server configuration

Run `iris lsp --stdio` to start the language server. The `lsp` subcommand is required;
`iris` alone no longer starts the server, and language-server options must follow `lsp`.
Supply startup settings as inline JSON or a UTF-8 JSON file:

```sh
iris lsp --stdio --config '{"diagnostics":{"onChange":true}}'
iris lsp --stdio --config-file ./iris.json
```

`--config` and `--config-file` are mutually exclusive and replace `--source-command` and
`--diagnostics-on-open`, `--diagnostics-on-save`, and `--diagnostics-on-change`. File paths are
relative to the process working directory, not the editor's workspace or the configuration file's
directory. Startup configuration files are read once and are not watched.

Editors that advertise the LSP `workspace.configuration` capability can provide the same settings
object in the `iris.server` workspace configuration section. Iris requests that section for the first
workspace folder after initialization and requests it again after each
`workspace/didChangeConfiguration` notification; the notification's `settings` value is only an
invalidation signal. Runtime settings take precedence over startup settings. Each response is a
complete runtime layer, so omitted or `null` fields inherit from the startup configuration rather
than from the preceding response. Invalid updates are shown in the editor and leave the last valid
configuration active. Source-setting updates rediscover and reconcile the loaded workspace without
discarding open buffers. Clients without workspace-configuration support continue using only the
startup configuration.

The defaults are:

```json
{
  "sources": { "kind": "spago" },
  "diagnostics": {
    "onOpen": true,
    "onSave": true,
    "onChange": false
  }
}
```

All settings are optional. Missing or `null` fields retain their defaults; `{}` and top-level
`null` also select the defaults. Unknown fields and invalid values are errors, reported on stderr
with exit status 2 before the LSP starts. Use the
[configuration JSON Schema](compiler-lsp/configuration/configuration.schema.json) for editor
validation; associate it through editor settings rather than adding a `$schema` property.

To replace `spago.lock` source discovery with a command:

```json
{
  "sources": {
    "kind": "command",
    "program": "spago",
    "arguments": ["sources"]
  }
}
```

`program` is an executable name or path containing a non-whitespace character; it is passed unchanged.
`arguments` is an optional array of individual strings (default `[]`). No shell parsing or expansion
occurs. The command runs in the server's process working directory and must print one source path or
glob per line; relative output paths are resolved from the first LSP workspace folder, falling back
to the process working directory.
Only use trusted configurations: source commands execute with the server's permissions.
Diagnostic settings control the corresponding document-event triggers, not all diagnostic publishing.

## Editor features

Iris provides code intelligence for PureScript projects through its VS Code extension.

<details>
<summary><strong>Completion</strong></summary>

![Completing a PureScript expression](.github/assets/vscode-demos/completion.gif)

</details>

<details>
<summary><strong>Automatic imports</strong></summary>

![Automatically importing a completed PureScript name](.github/assets/vscode-demos/automatic-import.gif)

</details>

<details>
<summary><strong>Live diagnostics</strong></summary>

![Updating diagnostics while editing PureScript](.github/assets/vscode-demos/live-diagnostics.gif)

</details>

<details>
<summary><strong>Inferred types</strong></summary>

![Viewing an inferred PureScript type](.github/assets/vscode-demos/inferred-types.gif)

</details>

<details>
<summary><strong>Go to definition</strong></summary>

![Navigating to a PureScript definition](.github/assets/vscode-demos/go-to-definition.gif)

</details>

<details>
<summary><strong>Find references</strong></summary>

![Finding references to a PureScript name](.github/assets/vscode-demos/find-references.gif)

</details>

<details>
<summary><strong>Rename</strong></summary>

![Renaming a PureScript name across files](.github/assets/vscode-demos/rename.gif)

</details>

<details>
<summary><strong>Document symbols</strong></summary>

![Searching symbols in a PureScript document](.github/assets/vscode-demos/document-symbols.gif)

</details>

<details>
<summary><strong>Workspace symbols</strong></summary>

![Searching PureScript symbols across a workspace](.github/assets/vscode-demos/workspace-symbols.gif)

</details>

<details>
<summary><strong>Typed-hole suggestions</strong></summary>

![Replacing a typed hole with an Iris suggestion](.github/assets/vscode-demos/typed-hole-suggestions.gif)

</details>

<details>
<summary><strong>Document highlights</strong></summary>

![Highlighting occurrences of PureScript names](.github/assets/vscode-demos/document-highlights.gif)

</details>

<details>
<summary><strong>Semantic highlighting</strong></summary>

![Enabling semantic highlighting for PureScript](.github/assets/vscode-demos/semantic-highlighting.gif)

</details>

## Installation

On Linux and macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/purefunctor/purescript-iris/main/install.sh | sh
```

On Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/purefunctor/purescript-iris/main/install.ps1 | iex
```

The installers verify the release's GitHub build-provenance attestation when
[GitHub CLI](https://cli.github.com/) is available. They display a warning and continue when it is not
installed. These installers require v0.1.0 or later; to install v0.0.x, use the installer from that
release's Git tag. Set `IRIS_VERSION` to a release tag or
`IRIS_INSTALL_DIR` to an installation directory to override the defaults.

Successful builds of the `main` branch are published as GitHub prereleases tagged
`v<version>-dev.<revision>`. Consumers testing against the canary channel should resolve the newest
published, non-draft prerelease and pass its exact tag through `IRIS_VERSION`. Stable installations
continue to use GitHub's latest release.

Iris keeps its package version separate from source provenance. Packagers can set
`IRIS_BUILD_REVISION` to a Git revision when invoking Cargo to include that revision in the reported
CLI and language-server versions. The value is read at compile time; builds that omit it report the
version from `compiler-bin/iris-cli/Cargo.toml` unchanged.

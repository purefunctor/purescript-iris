# Contributing

Thank you for taking interest in contributing to Iris.

## Development tools

[`mise.toml`](mise.toml) pins Rust, Node.js, pnpm, and the development CLIs.
With [mise](https://mise.jdx.dev/) installed, run `mise trust` and `mise install`
from the repository root, then use `mise exec -- just <recipe>` or activate mise
in your shell. `just format` uses the pinned nightly toolchain in this environment.

Amp orbs and Buildkite share `.agents/setup`, which bootstraps mise and installs
these tools. Native prerequisites remain platform-managed: a C/C++ toolchain,
pkg-config, OpenSSL development libraries, curl, and xz. Buildkite installs them
through apt on Linux or Homebrew on macOS (with Xcode command-line tools present).

Before pushing commits for a pull request, run `just format` and `just licenses`,
then squash any resulting changes into the relevant commits in the branch.

## Integration tests

Run `just t compiler` (alias `just t c`) for the unified compiler integration
category. It prepares the registry package set pinned in
[`tests-integration/packages.json`](tests-integration/packages.json)
before starting the fixture runner. Node.js 22 is required for JavaScript
execution. Lowering, resolving, and LSP are unchanged and do not load registry
packages.

For direct nextest use, prepare the packages first:

```sh
just integration-prepare
cargo nextest run -p tests-integration
```

Preparation downloads digest-verified archives into `target/integration-packages`.
Source sets are published atomically and are immutable; concurrent preparation
commands are serialized. Once prepared, fixtures read only local sources and run
offline. A warm preparation also requires no network. Do not put downloaded
sources into fixture directories.

### Fixture ownership and execution

Compiler fixtures live in `tests-integration/fixtures/compiler/` and enter through
`Main.purs`.

| Snapshot | Contents |
|----------|----------|
| `Main.checking.snap` | Checked types, kinds, and declaration metadata, without diagnostics |
| `Main.diagnostics.snap` | Parser, checking, foreign, and backend diagnostics for all fixture-owned modules, with stable fixture-relative paths |
| `Main.semantic.snap` | Checked semantic trees, including recovery |
| `Main.functional.snap` | A successful functional tree or an explicit rejection |

Each fixture runs all these reports; diagnostics do not skip later stages, which
also exercise compiler recovery.

Compiler fixtures compile their reachable dependency closure with the current
Iris. Only reachable fixture-owned generated JavaScript and adjacent FFI
are kept in `output/`; registry output and `runtime.js` are not goldens.

An optional `verify.mjs` is staged beside a fresh temporary `output/` containing
the complete generated program. Use imports such as `./output/Main/index.js` and
Node built-ins. The runner supplies ESM configuration; fixtures do not need a
`package.json`. Verification never executes tracked goldens. It must pass before
`just t compiler --update-output` writes new goldens.

Use real package modules rather than local library stand-ins. Tests that must
deliberately replace a compiler-known module can declare the module and its reason
in a fixture-local `replacements.json`:

```json
{
  "Data.Generic.Rep": "Omit representation types to test the missing representation diagnostic."
}
```

Undeclared collisions and unused replacements fail. Replacements apply only to
registry modules, never Prim or another fixture module, and do not inherit the
package's FFI.

### Updating dependencies

Edit `package_set` in [`tests-integration/packages.json`](tests-integration/packages.json)
to select a version from the [registry package sets](https://github.com/purescript/registry/tree/main/package-sets).
The same file lists the root packages. Then run:

```sh
just integration-prepare
```

Preparation automatically resolves the transitive closure using that package set,
checks dependency ranges, and obtains archive SHA-256 hashes from the registry.
The generated resolution lives only in `target/integration-packages/resolutions`;
there is no lockfile to create or commit. Changing the configuration selects a new
cache entry. A cold preparation needs registry access; warm preparation and fixture
execution remain offline. Review every resulting
fixture change: real package APIs, instances, and runtime representations may
differ from earlier versions. Run all affected categories without filters before
accepting the migration. Packages with FFI that imports additional assets need an
explicit staging design; the current runner copies adjacent FFI files only.

Registry packages are development dependencies, not compiler dependencies.
Release builds do not prepare them, and release archives must not include their
sources, FFI, or generated output. Cached packages retain their license files;
any third-party code retained in fixtures still needs its own provenance review.

## Agentic Coding

See [AGENTS.md](AGENTS.md) for agentic coding guidelines written for both humans and agents.

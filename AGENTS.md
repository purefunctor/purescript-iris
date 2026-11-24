## Design and scope

- Fix root causes in the underlying subsystem instead of patching symptoms or adding escape hatches.
- Refactor freely within the files you are changing.
- Discuss changes that cross crates or alter shared interfaces with the user before implementing them.
- Encode invariants in the type system where practical.
- Keep code locally understandable with explicit responsibilities so a reader can follow them without global context.
- Consolidate repeated logic within the code you touch when it clarifies a responsibility, and avoid speculative abstractions.
- Keep new features scoped to the requested use case without speculative failure handling or extra complexity.
- Trace changes across affected compiler layers (frontend, backend, services, and CLI) instead of stopping at the local crate.
- Report unrelated bugs or issues you discover instead of fixing them without asking.

## Code style

- Follow the existing conventions for variable names, argument ordering, module organization, and formatting.
- Use full, descriptive names for functions, variables, types, and modules instead of ad-hoc abbreviations; established domain acronyms and existing identifiers such as `id`, `uri`, or `lsp` are fine.
- Agents must not write new comments in any form: `//` and `/* */` comments, `///` and `/** */` item documentation, or `//!` and `/*! */` module documentation. Comments and documentation in Iris are written by humans.

Formatting and binding conventions:

```rust
// Yes: Keep collecting fluent when each call fits on one line.
let collection = source
    .map(|item| transform(item))
    .collect();

// No: Do not break immediately after `=`.
let collection =
    source.map(|item| transform(item)).collect();

// Yes: Bind before collecting when a fluent call spans multiple lines.
let collection = source.map(|item| {
    // ...
});

let collection = collection.collect();

// No: Do not collect directly from a multi-line fluent call.
let collection = source
    .map(|item| {
        // ...
    })
    .collect();

// Yes: Name concrete types in inherent implementations.
impl Span {
    pub fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }
}

// No: Do not use `Self` outside trait definitions and trait implementations.
impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

// Yes: Name meaningful intermediate results while keeping simple expressions inline.
let absolute_path = fs::canonicalize(&source.path)?;
let uri = Url::from_file_path(&absolute_path)
    .map_err(|_| Error::FileUrl(absolute_path.clone()))?
    .to_string();
let file_id = files.insert(uri, content.clone());
engine.set_content(file_id, content);

// No: Do not introduce an intermediate binding for every expression.
let source_path = &source.path;
let absolute_path_result = fs::canonicalize(source_path);
let absolute_path = absolute_path_result?;
let uri_result = Url::from_file_path(&absolute_path);
let uri_result = uri_result.map_err(|_| Error::FileUrl(absolute_path.clone()));
let uri = uri_result?.to_string();
let content = content.clone();
let file_id = files.insert(uri, content);
```

## Tests and verification

- Place each test where its behavior belongs, not where adding a `#[test]` is easiest:
  - Unit tests beside small algorithms and data structures, constructing fixture data directly when invariants do not need the full compiler engine (for example, functional dependency closure and exhaustiveness checking).
  - Compiler and internal pipeline tests in `tests-integration`.
  - CLI and external behavior tests in `tests-e2e`.
- Test behavior and invariants; do not write tautologies or change-detector tests for constants to raise coverage.

### Preflight checks

- Before shipping, push a temporary remote branch to run cross-platform CI, then delete the remote branch once the change ships.
- Skip preflight CI when cross-platform behavior cannot be affected (such as documentation or agent-config edits), and tell the user why.

## Commits

- Keep commits atomic and reviewable. Land branches on `main` through merge commits.
- Run `just format` and `just licenses` before merging a branch, and squash any resulting changes into the relevant commits.

### Commit format

Use short, imperative, sentence-cased titles:

```text
Add failing test case for overlapping instances
Fix inference for do expressions with final let
Implement local name completions
```

### Merge commit format

Use `[category] description` with one established category (check recent merge commits on `main` when unsure): a crate name for crate-local changes, `agents` for agent configuration, `meta` for repository-wide maintenance, or `ci` for CI changes. Do not use conventional commit qualifiers such as `fix` or `feat`.

Good:

```text
[checking] Preserve type variable names in instance members
[lsp] Handle rename rejections
[agents] Clarify merge commit subject categories
[ci] Test installers on supported platforms
```

Bad:

```text
Preserve type variable names in instance members       # Missing category
[fix] Preserve type variable names in instance members # Describes the change type, not the subsystem
[checking/lsp] Improve rename errors                   # Lists multiple categories
[misc] Update inference                                # Uses a vague category despite a clear subsystem
```

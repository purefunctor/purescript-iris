## Humans

Thank you for taking interest in contributing to Iris. We welcome contributions assisted by
agentic coding tools that follow these principles:

- **Justify your contribution.** As an external contributor, understand the problem your pull
  request solves and explain from the outset why it is worth solving in Iris, why your
  approach is appropriate, and how you verified it. Be prepared to discuss the tradeoffs and respond
  to review. Agents may help implement the change and write its description; responsibility for
  understanding and justifying the contribution remains yours. This expectation addresses external
  contributions, not a separate approval process for work directed by the project's author or
  maintainers.
- **Improve quality, not quantity.** Iris is a fast-moving project, but its maintainers are
  only human. We want to build a compiler for posterity, one that can withstand the test of time.
  Shipping features quickly can be tempting, but you should use those time savings to invest in
  improving quality.

PRs may be declined if these principles are not upheld.

## Agents

`AGENTS.md` and `.agents` are the canonical agent guidance. Use the relevant skills in
`.agents/skills` for task-specific workflows, `CONTRIBUTING.md` for contribution details, and the
repository's tooling for command discovery. These instructions supply maintainer intent rather than
a source-code map.

Apply workflows to the requested task, not as invitations to expand its scope. Explicit user
instructions take precedence over repository skill guidelines. If an instruction blocks requested
work, identify the exact instruction and the decision needed; continue independent authorized work.

## The author's ethos

### Correctness

- Investigate architectural root faults.
- Avoid escape hatches and temporary fixes.
- Use the type system to encode correctness.

### Posterity

- Write code for future contributors, reviewers, and maintainers.
- Write code that you will understand 10 years later.
- Write code that you will not hate 10 years later.

### Clarity

- Code should be self-documenting. Comments should say 'why', not 'what'.
- Never write narrative inline comments unless it is used to clarify intent.
- Never use abbreviated names for functions, variables, types, modules, etc.

### Simplicity

- Avoid abstractions for their own sake.
- Write abstractions if they improve clarity or reduce real complexity.
- Write abstractions if they make repeated work easier for humans.

## Applying the principles

Fix the cause in the layer that owns it. Make the smallest coherent change that restores the
invariant without unrelated architectural cleanup. Existing code establishes conventions, not
necessarily correct behavior. Resolve routine implementation details from the source; ask when
unresolved language or product intent would change the outcome rather than silently choosing new
behavior.

## Code style

In addition to the author's ethos, follow the project's existing conventions for variable names,
argument ordering, module organisation, and formatting.

For example:

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

Use the `writing-code-commentary` skill for non-obvious compiler derivations and algorithm traces.

## Test ownership

Choose the test level by the behavior under test, not by how easy it is to add a Rust `#[test]`.

- Unit tests belong beside small algorithms and data structures within a subsystem. Construct local
  data directly when the invariant can be tested without loading source and driving compiler stages;
  functional-dependency closure and pattern-matrix operations are examples.
- Source-file behavior through compiler APIs belongs in `tests-integration`: loading, parsing,
  resolving, checking, code generation, and editor analysis. Use the existing fixture harness
  instead of assembling a second compiler pipeline inside a unit test.
- CLI, Spago, shell/process, and development-environment behavior belongs in `tests-e2e`.

Do not add a unit test that repeats a fixture's behavioral coverage. Tests at multiple levels should
protect distinct contracts, such as a local algorithm invariant and its integration into
compilation. Choose tests for their regression value and maintenance cost, not test counts or line
coverage.

## Verification

Choose checks by affected behavior, not just changed paths. Scale verification to the change; after
required checks pass, broaden or repeat them only for new changes, failures, or unresolved concerns.

- Check changed Rust crates with `cargo check -p <crate-name> --tests`; package scope is mandatory.
- Run crate unit tests with `cargo nextest run -p <crate-name>`.
- Use `just t <category> [filters...]` for integration tests. Before pushing a change that affects
  integration tests, run every affected category without filters and confirm that each passes with
  no pending snapshots. Filtered runs are for iteration, not the final gate.
- Never edit `.snap` files or generated JavaScript goldens by hand. Regenerate them through their
  owning commands and inspect every changed expectation. Accept only changes that describe the
  intended behavior, including deliberately recorded buggy behavior in a regression-test commit.
- Snapshots record observed behavior; passing or accepting them does not establish semantic
  correctness. Check the result against the intended behavior.
- Run `ast-grep scan` on changed Rust files and fix new findings; it exits non-zero on
  error-severity rules. After changing `rules/`, run `ast-grep test`.
- Use `just format` for Rust formatting; it requires nightly and sets the required import
  granularity.

## Commits and pull requests

Commits must be atomic units of work. Pull requests use merge commits that retain branch history;
curate the branch into a reviewable story before opening a PR to avoid force-push noise.

### Commit format

Regular commits use a short imperative, sentence-case subject naming the behavior or subsystem, not
the PR title format:

```text
Add failing test case for overlapping instances
Fix inference for do expressions with final let
Implement local name completions
```

### Pull request title format

PR titles use `[category] description`. GitHub appends the PR number; do not include it yourself.
Choose the narrowest established category for the primary subsystem or project area, consulting
recent merge commits on `main` when necessary. Use crate names such as `checking` or `analyzer`, or
broader areas such as `lsp`; use `agents` for agent configuration, `meta` for repository-wide
maintenance, and `ci` for CI changes. Do not use change types such as `fix`, vague categories such
as `misc`, or multiple categories.

Good pull request titles:

```text
[checking] Preserve type variable names in instance members
[lsp] Handle rename rejections
[agents] Clarify pull request title categories
[ci] Test installers on supported platforms
```

Bad pull request titles:

```text
Preserve type variable names in instance members       # Missing category
[fix] Preserve type variable names in instance members # Describes the change type, not the subsystem
[checking/lsp] Improve rename errors                   # Lists multiple categories
[misc] Update inference                                # Uses a vague category despite a clear subsystem
```

## Ethos

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
- Never write narrative inline comments unless they are used to clarify intent.
- Never use abbreviated names for functions, variables, types, modules, etc.

### Simplicity

- Avoid abstractions for their own sake.
- Write abstractions if they improve clarity or reduce real complexity.
- Write abstractions if they make repeated work easier for humans.

## In context of work

Assume in good faith that maintainers will continue to read through and review the code. Unlike a
frontend project, where you can verify irrefutable proof via the end product, e.g., a working
feature, compilers are deeply tied to the code that they generate and the code contributors write
for the compiler itself. Recall the ethos above when writing code that humans will review.

Remember that compilers are both horizontal and vertical: frontend, backend, services that build on
top of them, and ultimately the command line interface, which is what humans and agents interact
with daily.

Posterity is the act of authoring code for future readers, human and agent alike. It’s not a
requirement to anticipate every failure mode and to address them all at once. Humans and agents
strongly rely on working memory and local understanding of a large system. Overload is fatigue.

For new features, thoughtfully consider the proportionality between complexity and benefit.
Following through to correctness and posterity does not authorise fixing unrelated discoveries on a
whim without first informing the user about these discoveries.

When addressing a bug, investigate the underlying systems instead of stopping at the point where the
bug is visible. Thoughtfully consider whether fixing the bug constitutes a larger refactoring that
you should raise with the user for a discussion.

## Code style

In addition to the ethos, follow the project's existing conventions for variable names, argument
ordering, module organisation, and formatting.

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

### Comments

Agents must not write new comments in any form: `//` and `/* */` comments, `///` and `/** */` item
documentation, or `//!` and `/*! */` module documentation. Comments and documentation in Iris are
written by humans.

## Tests and verification

Thoughtfully create tests by the behaviour under test, not by how easy it is to add a Rust
`#[test]`. Aligned with our warning about correctness and posterity, do not blindly chase test
coverage to the point that you write tautologies or change detector tests for constants.

Unit tests belong beside small algorithms and data structures within a subsystem. Construct fixture
data directly when the invariant can be tested without driving the full compiler engine. You can see
this in the functional dependency closure computation and exhaustiveness checker algorithms.

Testing internal behaviour such as the compiler belongs in the `tests-integration` crate. Testing
external behaviour such as the command line belongs in the `tests-e2e` crate.

### Preflight checks

Before shipping a change, perform a preflight check. You can push to a remote branch to build and
test the project against other supported platforms using CI. You may skip preflight checks if you
are confident that the change does not need cross-platform verification; tell the user if you chose
not to and why. Delete the remote branches you create once the changes ship.

## Commits

Commits must be atomic units of work. Branches land on `main` through merge commits. Thoughtfully
curate branches into reviewable stories before publishing them. Always run `just format` and
`just licenses` before merging a branch, and squash any resulting changes into the relevant commits.

### Commit format

Commit titles are short, imperative, and sentence-cased:

```text
Add failing test case for overlapping instances
Fix inference for do expressions with final let
Implement local name completions
```

### Merge commit format

Merge commit titles use `[category] description`. Thoughtfully choose from established categories,
consulting recent merge commits on `main` where necessary. Use crate names when changes are local to
the crates themselves; `agents` for agent configuration; `meta` for repository-wide maintenance; and
`ci` for changes. Do not use conventional commit qualifiers such as `fix` or `feat`.

Good merge commit subjects:

```text
[checking] Preserve type variable names in instance members
[lsp] Handle rename rejections
[agents] Clarify merge commit subject categories
[ci] Test installers on supported platforms
```

Bad merge commit subjects:

```text
Preserve type variable names in instance members       # Missing category
[fix] Preserve type variable names in instance members # Describes the change type, not the subsystem
[checking/lsp] Improve rename errors                   # Lists multiple categories
[misc] Update inference                                # Uses a vague category despite a clear subsystem
```

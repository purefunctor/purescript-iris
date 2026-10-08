# Formatting

Iris formats complete PureScript modules from a Spago workspace, explicit files, or an editor buffer.

## Selecting files

Run `iris format` to format every package's `src` and `test` sources in place. It uses the same
workspace discovery as builds, even from inside a package. Fetched dependencies, symlinks, nested
workspaces, and files outside workspace package sources are excluded. No fetching or compilation
is needed.

Use `--file PATH` for tools and standalone files. Paths are literal: directories and glob patterns
are not expanded, and workspace discovery is bypassed. A single file prints to stdout by default.
Repeat `--file` to select multiple files, with `--write` or `--check` to avoid ambiguous output.

Use `--file -` to read stdin explicitly; bare `iris format` always selects the workspace.
Stdin cannot be combined with `--write`.

```sh
iris format
iris format --check
iris format --file src/Main.purs
iris format --write --file src/Main.purs --file test/Main.purs
cat src/Main.purs | iris format --file -
iris format --width 100 --indent 4 --unicode
```

## Options

| Option | Behavior |
|--------|----------|
| `--check` | List unformatted paths and exit with status 1 when changes are needed, without modifying files |
| `--write` | Replace selected files with formatted output; the default in workspace mode |
| `--width COLUMNS` | Preferred line width; a positive value, defaulting to 100 |
| `--indent SPACES` | Spaces per indentation level; from 1 to 65535, defaulting to 2 |
| `--unicode` | Emit Unicode built-in signatures, arrows, constraints, and quantifiers |

`--write` and `--check` are mutually exclusive. Unicode conversion uses `∷`, `←`, `→`, `⇐`, `⇒`,
and `∀`, and happens before width-aware layout. Literals, comments, record labels, and user-defined
operators are not rewritten; in particular, `<=` used as an operator stays `<=`. Without the flag,
source spellings are preserved, including existing Unicode.

## Layout conventions

Ordinary source wrapping does not determine output wrapping. Outside the import block, section
separation and comment placement are retained. The formatter wraps applications, signatures, types,
and delimited lists at the preferred width, with these conventions:

- Function and constraint arrows stay at the ends of wrapped lines, with successive type operands
  aligned. Other infix operators, data-constructor pipes, and functional-dependency arrows lead
  continuation lines. `forall` periods stay with the quantified variables.
- Nonempty layout blocks expand. `do` and `ado` stay inline when their headers fit; `let-in` results
  sit beneath `in`.
- Wrapped records, record updates, arrays, and rows use leading commas aligned with the opening
  and closing delimiters. The first item stays beside the opener unless a comment forces a newline.
  Items start two columns after the delimiter; their continuations use `--indent`.
- Nonempty arrays and array binders have whitespace inside their brackets; empty arrays stay `[]`.
- Bindings in expression `where` blocks align with `where`. `let`, class, and instance bodies retain
  their normal indentation.
- Imports are sorted by module name. An open, unaliased `import Prelude` comes first, separated from
  the remaining imports by a blank line. Explicit, hiding, and aliased Prelude imports sort with
  other imports. Imports of the same module retain their relative order; import lists are not sorted
  or merged. Leading and trailing comments move with their imports.

Literals and qualifiers are preserved verbatim. Comment text is preserved except that CRLF line
endings normalize to LF. Indivisible atoms and comments can exceed the preferred width.

## Validation and exit codes

Each result is validated against the original token kinds, expected spellings, comments, and layout
structure. All inputs are validated before any writes. Changed files are written in place, retaining
file identity, ownership, access-control lists, and hardlink relationships. Normal OS write semantics
apply, including possible clearing of set-ID bits. Writes are not atomic: an interrupted or failed
write can leave partial contents, and a batch is not a filesystem transaction.

The CLI intentionally uses `std::fs::write` on trusted workspace paths. `--write` refuses paths
identified as symlinks during preflight; this is a convenience safeguard, not a security boundary.
Concurrent path substitution is outside the formatter's guarantees. Atomic replacement and custom
platform metadata management are also outside its scope.

| Status | Meaning |
|--------|---------|
| 0 | Formatting succeeded, or `--check` found no changes |
| 1 | `--check` found files that need formatting |
| 2 | Invalid options, invalid source, failed validation, or a file/workspace error |

## Editor formatting

Editor **Format Document** uses the same formatter on unsaved content after the language server's
normal workspace preparation. The request's `tabSize` sets indentation; an optional Iris extension,
`options.lineWidth`, sets the preferred width. Output uses spaces even when `insertSpaces` is false.
Invalid documents receive no edits, and invalid options are rejected.

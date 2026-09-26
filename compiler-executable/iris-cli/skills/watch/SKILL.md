---
name: watch
description: Ask a running `iris watch` about a PureScript project with `iris watch query`: signatures, module exports, definitions, references, diagnostics, and generated JavaScript, addressed by qualified name. Use when working in a project built with Iris, after editing PureScript files, or instead of reading dependency sources to learn an API.
---

# Querying `iris watch`

`iris watch` keeps the whole project compiled in memory, dependencies included. `iris watch query`
asks it questions by qualified name, so you do not need to open files or start a second compiler.

## Find the watcher

Run queries from anywhere inside the project. If the watcher was started with `--output DIR`, pass
the same option: append `query …` to the watcher's command line (`iris watch --output DIR query
wait`), or run `iris watch query --output DIR wait`. A relative `DIR` is resolved from the current
directory, so run the query from the directory the watcher was started in.

```bash
iris watch query wait
```

- Exit status 4 means no watcher is running for the project's output directory. Before starting
  one, check whether the project already starts it: look at `package.json` scripts (a `dev` script
  may run `iris watch` next to a dev server), `.amp/services.yaml`, and the project's agent guide.
  If a service runs it, start or restart that service instead. Otherwise run `iris watch` in the
  background and keep it running; queries right after it starts exit 4 until Spago has fetched
  dependencies, so retry for a few seconds.
- Never stop or restart a watcher you did not start. Only one watcher can run per output directory.
- A warning that the watcher runs a different Iris version means the project uses another `iris`
  build, which the warning names; use that executable for queries if answers look wrong.

## After editing files

```bash
iris watch query wait
iris watch query diagnostics
```

`wait` rescans the sources, rebuilds if anything changed, and reports whether the build succeeded.
Always run it after writing files and before other queries, so answers reflect your edits. It exits
0 even when the build has errors; read its output, then ask for `diagnostics`.

## Queries

| Command | Answer |
|---|---|
| `iris watch query wait` | Build outcome after picking up every change on disk |
| `iris watch query signature Data.Maybe.fromMaybe` | Signature of a value, or kind of a type or class, with its documentation |
| `iris watch query module Data.Maybe` | Every export of a module with signatures and documentation |
| `iris watch query definition Data.Maybe.Maybe` | `path:line:column` of the declaration |
| `iris watch query references Data.Maybe.fromMaybe` | `path:line:column` of every use |
| `iris watch query diagnostics [Main]` | Errors and warnings of every module, or of one module |
| `iris watch query javascript Main` | The JavaScript written to `output/` for a module |

- Names are fully qualified: the module, a dot, then the item. Operators may be written with or
  without parentheses: `Data.Function.(<<<)`.
- A name that is both a type and a value, such as a constructor named after its type, answers both.
  To ask about one, put `value` or `type` before the name: `iris watch query definition type
  Data.Maybe.Maybe`, `iris watch query signature value Data.Tuple.Tuple`. This works for `signature`,
  `definition`, and `references`; classes are in the `type` namespace, and data constructors and
  class members in the `value` namespace.
- Any module in the project or its dependencies can be queried, including ones that are not
  imported anywhere.
- Declarations of the built-in `Prim` modules point at copies the watcher writes to a temporary
  directory, which you can read like any other source.
- Paths inside the project, dependencies included, are relative to its root; lines and columns
  count from 1, and columns count characters.
- `javascript` fails while any module has errors, because nothing is written to `output/` then.
- `--json` prints the watcher's response as JSON: `{"id", "kind", "generation", "value"}`.
- Exit status: 0 answered, 1 the query failed (the message says why, for example an unknown name),
  2 invalid arguments, 4 no watcher.

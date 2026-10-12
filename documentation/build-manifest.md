# Build manifest

`iris build`, `iris run`, `iris test`, and `iris watch` write `iris-manifest.json` into the output
directory beside the generated modules. Tools such as bundlers read it to learn which package owns
each module, where its source is, which modules it imports, and whether the runtime module is
needed, without guessing from module names or talking to a running watcher.

## Format

```json
{
  "version": 1,
  "iris": "0.1.7",
  "workspace_root": "/home/user/project",
  "runtime": null,
  "packages": {
    "application": { "editable": true, "root": "/home/user/project", "dependencies": ["library"] },
    "library": { "editable": true, "root": "/home/user/project/packages/library", "dependencies": [] }
  },
  "modules": {
    "Library": {
      "package": "library",
      "source": "/home/user/project/packages/library/src/Library.purs",
      "javascript": "Library/index.js",
      "foreign": "Library/foreign.js",
      "stylex_theme": null,
      "dependencies": []
    },
    "Main": {
      "package": "application",
      "source": "/home/user/project/src/Main.purs",
      "javascript": "Main/index.js",
      "foreign": null,
      "stylex_theme": null,
      "dependencies": ["Library"]
    }
  }
}
```

| Field | Meaning |
|-------|---------|
| `version` | The schema version. It increases when a change could break an existing reader. |
| `iris` | The version of the Iris that wrote the manifest, such as `0.1.7` or `0.1.7-dev.0123abc` for a canary build. |
| `workspace_root` | The Spago workspace root. |
| `runtime` | The runtime module, or `null` when no generated module needs it. |
| `packages` | The packages of the build: the selected packages and their dependencies, including packages that have no modules. |
| `packages.*.editable` | Whether the package's sources are project files, such as workspace and local packages, rather than a fetched checkout. |
| `packages.*.root` | The package directory. |
| `packages.*.dependencies` | The names of the package's dependencies. |
| `modules` | Every module the build wrote. Built-in `Prim` modules are never written, so they never appear. |
| `modules.*.package` | The package that owns the module's source. |
| `modules.*.source` | The PureScript source file. |
| `modules.*.javascript` | The generated module. |
| `modules.*.foreign` | The copied foreign module, or `null`. |
| `modules.*.stylex_theme` | The generated StyleX theme module, or `null`. |
| `modules.*.dependencies` | The names of the modules the generated JavaScript imports. |

`workspace_root`, package `root`, and module `source` are absolute. `runtime`, `javascript`,
`foreign`, and `stylex_theme` are relative to the output directory. Every path uses forward slashes
on every platform. Object keys and lists are sorted.

A source file belongs to the package whose source globs match it, which are `src/**/*.purs` and
`test/**/*.purs` under the package directory. A build reports a file that two packages claim as an
error. `iris watch` applies the same rule to files created while it runs, and reports a conflict
without changing anything.

## When it is written

A build writes its generated files, `iris watch` removes the files its previous manifest named and
the new one does not, and the manifest is replaced last, through a temporary file and a rename. A
reader never sees a partially written manifest, and when it sees a new one, every file it names
exists with the content of that build.

The manifest is replaced exactly when the output changed. An edit that only changes generated code,
such as a function body, replaces it too, so a tool that watches the manifest learns about every
change to the output.

A build with errors writes nothing, so the output and the manifest stay at the last successful
build together. Errors are not recorded in the manifest. `iris build --resilient` writes modules
despite errors, and writes a manifest for them.

`iris build` does not know what an earlier build wrote, so it never removes files.

`iris watch` keeps the last manifest it wrote as its record of the output directory:

- A rebuild with errors keeps that record without the modules whose sources were deleted or
  renamed, and removes their files. A kept module's `dependencies` name only modules that still
  exist, as the compiler resolves imports; a dependency on a removed module is dropped, although
  the kept JavaScript still imports it until a successful rebuild.
- A watcher with no current build has no manifest. There is none while the first build has errors
  or while there are no input files, and a manifest left by an earlier build is removed then. The
  files of an earlier build are not the watcher's, so they stay.
- A write that fails partway removes the files it created, puts back the files it replaced, and
  leaves the manifest as it was.

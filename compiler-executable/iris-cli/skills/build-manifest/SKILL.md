---
name: build-manifest
description: Read `iris-manifest.json`, the versioned manifest that `iris build` and `iris watch` write into the output directory, to find which package owns each generated module, whether it is project code or a fetched dependency, its source file, its foreign and StyleX files, and the modules it imports. Use when writing or changing a bundler plugin, dev server, or other tool that consumes Iris output, or when deciding whether a generated module is local code or a library.
---

# The Iris build manifest

After every build that writes output, Iris writes `iris-manifest.json` into the output directory
(`output/` unless the build was given `--output DIR`). It lists the modules that build wrote, the
package that owns each one, and the packages of the build. Read it instead of guessing ownership
from module names, scanning `import` statements, or asking a running watcher.

```json
{
  "version": 1,
  "iris": "0.1.6",
  "workspace_root": "/home/user/project",
  "runtime": null,
  "packages": {
    "application": {
      "editable": true,
      "root": "/home/user/project",
      "dependencies": ["library"]
    },
    "library": {
      "editable": true,
      "root": "/home/user/project/packages/library",
      "dependencies": []
    }
  },
  "modules": {
    "Main": {
      "package": "application",
      "source": "/home/user/project/src/Main.purs",
      "javascript": "Main/index.js",
      "foreign": "Main/foreign.js",
      "stylex_theme": null,
      "dependencies": ["Library"]
    }
  }
}
```

## Fields

- `version` is the schema version. It increases when a change could break a reader, so check it
  and stop with a clear error when it is not a version your tool knows.
- `iris` is the version of Iris that wrote the file.
- `workspace_root` is the Spago workspace root, the directory whose `spago.yaml` has a
  `workspace` section.
- `runtime` is the shared runtime module, or `null` when no generated module imports it.
- `packages` is keyed by package name: `editable` is `true` for workspace and local packages and
  `false` for packages Spago fetched; `root` is the package directory; `dependencies` are package
  names, each a key of `packages`.
- `modules` is keyed by module name: `package` is a key of `packages`; `source` is the PureScript
  file; `javascript`, `foreign` (`foreign.js` or `foreign.jsx`), and `stylex_theme` are generated
  files, the last two `null` when absent; `dependencies` are the module names its JavaScript
  imports. Built-in `Prim` modules have no output and are never listed.
- With a selected package (`--package`, or running inside a package directory), `packages` and
  `modules` cover that package and what it depends on.

## Paths

- `workspace_root`, `root`, and `source` are absolute, so they work wherever the output directory
  is, including with `--output DIR`.
- `javascript`, `foreign`, `stylex_theme`, and `runtime` are relative to the output directory,
  the directory that contains the manifest.
- Paths use forward slashes on every platform, including Windows, where an absolute path looks
  like `C:/Users/user/project`.

## When it changes

- Iris writes the manifest after the generated modules and replaces it in one step, so a reader
  never sees a partial file. A changed manifest means a build finished writing its output; reload
  from it rather than from individual file events in `output/`.
- Because the file is replaced, not edited, watch the output directory for its name rather than
  the file itself.
- An unchanged build leaves the file untouched.
- A build with errors writes nothing, so the manifest and the output stay at the last successful
  build. Report the errors from `iris watch query diagnostics` (see `iris skills get watch`); the
  manifest does not record them.
- After a watch rebuild with errors, modules whose sources were deleted or renamed are removed from
  both the output and the manifest. A remaining module may then list a dependency that `modules`
  does not contain, just as its JavaScript imports a file that is gone.
- With no input files, `iris watch` removes its output, including the manifest. A missing manifest
  means there is no output to read.

## Common uses

- Project code versus libraries: `packages[modules[name].package].editable`.
- The modules of one package: filter `modules` by `package`.
- The module graph: follow `dependencies` instead of parsing JavaScript.
- Mapping a generated file back to its source: match `javascript` against the path relative to
  the output directory, then read `source`.

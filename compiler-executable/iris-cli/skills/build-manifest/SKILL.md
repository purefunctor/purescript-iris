---
name: build-manifest
description: Read `output/iris-manifest.json`, which `iris build`, `iris run`, `iris test`, and `iris watch` write beside the generated JavaScript, to learn which package owns each module, where its source is, which modules it imports, and whether the runtime module is needed. Use when writing a bundler plugin, dev server, or other tool that consumes Iris output, or when deciding whether a module is project code or library code.
---

# Reading the build manifest

Every successful Iris build writes `iris-manifest.json` into its output directory, `output/` unless
the build was started with `--output DIR`. Read it instead of guessing from module names or
listing the output directory.

```json
{
  "version": 1,
  "iris": "0.1.7",
  "workspace_root": "/home/user/project",
  "runtime": null,
  "packages": {
    "application": { "editable": true, "root": "/home/user/project", "dependencies": ["library"] }
  },
  "modules": {
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

- `version` is the schema version; refuse a version you do not know. `iris` is the version of the
  Iris that wrote the file.
- `workspace_root`, package `root`, and module `source` are absolute. `runtime`, `javascript`,
  `foreign`, and `stylex_theme` are relative to the output directory. Paths use forward slashes
  on every platform.
- `modules` lists every module the build wrote, project and dependency modules alike, and never
  the built-in `Prim` modules. `dependencies` names the modules a module's generated JavaScript
  imports. `runtime` is the shared runtime module when any module needs it, or `null`.
- `packages` lists the selected packages and their dependencies. A package is `editable` when its
  sources are project files, such as workspace and local packages, and not a fetched checkout.

## Telling project code from library code

Look the module up in `modules`, then its package in `packages`:

```js
const manifest = JSON.parse(await readFile("output/iris-manifest.json", "utf8"));
const isProjectModule = (name) => manifest.packages[manifest.modules[name].package].editable;
```

Do not derive this from module name prefixes or from the directories in `output/`.

## Following changes

- The manifest is replaced through a rename exactly when the output changed, after the generated
  files were written and files that no longer belong were removed. Watch the manifest and re-read
  it when it changes; every file it names exists then. An edit that only changes generated code
  replaces the manifest too, even though its content stays the same.
- A build with errors writes nothing, so the output and the manifest stay at the last successful
  build. Errors are not recorded; ask `iris watch query diagnostics` for them (see
  `iris skills get watch`).
- `iris watch` has no manifest while it has no current build: while its first build has errors or
  there are no input files. Treat a missing manifest as "no build yet", not as an empty project.
- While `iris watch` rebuilds with errors, the manifest keeps the last successful build without
  the modules whose sources were deleted or renamed. Every name in `dependencies` has an entry in
  `modules`; a dependency on a removed module is dropped with it.
- `iris build --resilient` writes modules despite errors, and a manifest for them.

The full format is documented in `documentation/build-manifest.md` of the Iris repository.

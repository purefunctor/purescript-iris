# Build manifest

`iris build`, `iris run`, `iris test`, and `iris watch` write `iris-manifest.json` into the output
directory (`output/` by default). The manifest describes the modules that the build wrote, so tools
such as bundlers can tell which package owns each generated module without talking to a running
watcher.

## When the manifest is written

A build writes the manifest after it writes the generated modules. It replaces the file in one
step, so a reader never sees a partial manifest. If the content did not change, the file is left
as it is. A change to the manifest therefore means that a build finished writing its output.

A build with diagnostics writes no modules and no manifest, so the output directory and the
manifest stay at the last successful build together. `iris build --resilient` writes modules
despite diagnostics, and it writes a manifest that describes them.

`iris watch` keeps the manifest in step with the files it owns:

- After a rebuild with diagnostics, the watcher removes the output of modules whose sources were
  deleted or renamed. It also removes those modules from the manifest.
- When no input files remain, the watcher removes all of its output, including the manifest.

## Example

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
    "Library": {
      "package": "library",
      "source": "/home/user/project/packages/library/src/Library.purs",
      "javascript": "Library/index.js",
      "foreign": null,
      "stylex_theme": null,
      "dependencies": []
    },
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

- `version`: the manifest schema version. It increases when a change could break an existing
  reader.
- `iris`: the version of Iris that wrote the manifest.
- `workspace_root`: the Spago workspace root, the directory whose `spago.yaml` has a `workspace`
  section.
- `runtime`: the shared runtime module, or `null` when no generated module imports it.
- `packages`: every package in the build, keyed by package name. When the build selects a package,
  this is that package and the packages it depends on.
  - `editable`: `true` for workspace and local packages, `false` for packages that Spago fetched.
  - `root`: the package directory.
  - `dependencies`: the names of the packages it depends on. Each name is a key in `packages`.
- `modules`: every module the build wrote, keyed by module name. Built-in `Prim` modules have no
  output and are not listed.
  - `package`: the name of the package that owns the source. It is a key in `packages`.
  - `source`: the PureScript source file.
  - `javascript`: the generated module.
  - `foreign`: the copied foreign module (`foreign.js` or `foreign.jsx`), or `null`.
  - `stylex_theme`: the generated StyleX theme module, or `null`.
  - `dependencies`: the names of the modules that the generated module imports. After a
    `--resilient` build or a watch rebuild with diagnostics, a dependency might not be listed in
    `modules`.

Keys and lists are sorted.

## Paths

`workspace_root`, `root`, and `source` are absolute, so a reader can find sources wherever the
output directory is. `runtime`, `javascript`, `foreign`, and `stylex_theme` are relative to the
output directory, which is the directory that contains the manifest. Paths use forward slashes on
every platform, including Windows, where an absolute path looks like `C:/Users/user/project`.

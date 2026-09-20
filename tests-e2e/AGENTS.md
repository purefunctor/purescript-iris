## Ownership

This crate tests the compiler as a tool in a development environment: command-line behavior, Spago
integration, working directories, generated project files, child processes, and watch/run workflows.
Exercise the real command and relevant external tools through the existing temporary-workspace
harness. Do not replace the boundary under test with direct compiler-library calls or mocks.

Compiler source semantics belong in `tests-integration`. Keep source programs here only as large as
needed to exercise the CLI or environment contract; do not recreate the compiler fixture suite.

## Assertions and isolation

- Check observable outcomes such as exit status, output, filesystem effects, and tool invocations. A
  successful process exit alone does not prove the requested operation happened.
- Use Insta snapshots for diagnostic output rather than collections of substring assertions or a new
  snapshot mechanism. Normalize incidental paths and platform differences without hiding meaningful
  diagnostic changes. Path normalization must account for raw and canonical workspace spellings,
  native and URI separators, and platform-specific process status text. Match complete path prefixes
  so similarly named paths remain visible. Keep behavioral assertions alongside snapshots where
  needed.
- Reuse temporary workspaces and the existing process/tool setup. Keep tests independent of a
  developer's checkout and global configuration, and clean up child processes started by a test.
- Preserve cross-platform behavior; account for path and executable differences in the shared
  harness rather than weakening assertions on one platform.

The `just e2e-prepare` and `just e2e` recipes own tool preparation and execution. These tests have
different environmental requirements from compiler fixtures; do not move them into unit tests merely
to avoid that setup.

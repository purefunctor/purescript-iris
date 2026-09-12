_default:
    just --list

set positional-arguments

[doc("Generate coverage for local tests")]
coverage:
  cargo llvm-cov clean --workspace
  just integration-prepare
  cargo llvm-cov nextest --no-report
  cargo llvm-cov nextest --no-report -p tests-integration

[doc("Generate coverage with the package set")]
coverage-full: coverage
  cargo llvm-cov nextest --no-report -p tests-compatibility

[doc("Generate coverage report for Codecov")]
coverage-codecov:
  cargo llvm-cov report --codecov --output-path codecov.json

[doc("Generate coverage report as HTML")]
coverage-html:
  cargo llvm-cov report --html

[doc("Prepare locked registry sources for integration tests")]
@integration-prepare:
  cargo run -q -p tests-support -- prepare tests-integration/packages.json target/integration-packages

@integration *args="": integration-prepare
  cargo nextest run -p tests-integration "$@" --status-level=fail --final-status-level=fail --failure-output=final

[doc("Install end-to-end test tools")]
@e2e-prepare:
  pnpm --dir tests-e2e/tools install --frozen-lockfile

[doc("Run end-to-end tests")]
@e2e *args="":
  cargo nextest run -p tests-e2e -j 1 "$@"

[doc("Run integration tests with snapshot diffing: compiler (c)|lowering (l)|resolving (r)|lsp")]
@t *args="":
  cargo run -q -p compiler-scripts --release -- "$@"

[doc("Run package-set benchmarks (e.g. just bench --bench checking_single_core)")]
@bench *args="":
  cargo criterion -p tests-compatibility "$@"

[doc("Compare package compatibility with a base revision using release builds")]
@compatibility base="origin/main":
  bash .agents/skills/running-compatibility-checks/scripts/run.sh {{quote(base)}}

[doc("Apply clippy fixes and format")]
fix:
  cargo clippy --workspace --fix && cargo fmt

[doc("Update THIRDPARTY.toml")]
[working-directory: 'compiler-bin/iris-cli']
licenses:
  cargo bundle-licenses --prefer MIT -o ../../THIRDPARTY.toml

[doc("Update the release version and third-party licenses")]
prepare-release version:
  cargo set-version --package iris-cli "{{version}}"
  just licenses

[doc("Format imports with module granularity")]
@format *args="":
  cargo +"${IRIS_NIGHTLY_TOOLCHAIN:-nightly}" fmt {{args}} -- --config imports_granularity=Module

[doc("Regenerate the language server configuration JSON Schema")]
@configuration-schema:
  cargo run -q -p configuration --features schema --example export-schema

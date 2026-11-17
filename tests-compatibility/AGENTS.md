## Ownership

This crate tests compatibility against real package-set source corpora and provides compiler
benchmarks. It complements focused integration fixtures; it is not the home for another fixture
runner or source-based compiler unit tests.

When a package reveals a compiler bug, capture the specific behavior in `tests-integration` and use
the package corpus to check its broader impact. Keep unit tests here focused on local verifier and
registry infrastructure contracts, with constructed data where appropriate.

## Evidence

Compare the same package selection and configuration when assessing a compiler change. Distinguish
preparation or environment failures from compiler incompatibilities; do not silently drop failing
packages to improve the report. Use the `running-compatibility-checks` skill for release-built
comparisons and the `measuring-performance` skill for performance measurements. Compatibility
results and performance measurements answer different questions; neither test counts nor line
coverage establish either result.

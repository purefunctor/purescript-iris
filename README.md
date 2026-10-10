<h1 align="center"><a href="https://iris-lang.com"><img src=".github/assets/iris-readme-banner.webp" alt="Iris" width="1200"></a></h1>
<p align="center">a language implementation for PureScript</p>

> [!WARNING]
> **Iris is alpha software.** Documentation is sparse, and behavior will change. Expect rough edges
> and breaking changes as the compiler and language server evolve.

---

Iris is a language implementation for PureScript, powered by an incremental, query-based build
system. Instead of a sequence of compiler phases, Iris models compilation and semantic information
as incrementally computed queries. These queries are used extensively to implement code intelligence
features in the language server.

The build system is designed with interactive editing in mind. To support this, it tracks dependencies
between inputs and queries, caches query results, deduplicates in-progress work across threads, and
supports cooperative cancellation when inputs change. Crucially, many query results are designed to
be incrementally reusable. For example, the compiler uses stable identities in lieu of source ranges 
to enable minimal recomputation across trivial formatting changes.

The language server component implements core code intelligence features such as completion, jump to
definition, hover information, find references, workspace symbol search, and diagnostics.

## Documentation

The [Iris documentation](https://iris-lang.com/docs) covers installing and using Iris:

- [Getting started](https://iris-lang.com/docs/getting-started): install Iris and run your first
  application.
- [Installation](https://iris-lang.com/docs/installation): installer options, release verification,
  canary builds, and packaging.
- [Language server](https://iris-lang.com/docs/language-server): set up Visual Studio Code or
  another editor, configure diagnostics, and understand workspace preparation.
- [Formatting](https://iris-lang.com/docs/formatting): format sources with `iris format` or from
  your editor.

# JavaScript output: runtime representations and differential reviews

This is a guide to the **JavaScript Iris emits**, not to the compiler's internal
architecture or to a stable cross-compiler ABI. It describes what to look for in
`output/<Module>/index.js` when reviewing output against purs. These shapes are
current output conventions, not a promise that purs produces the same shapes or
that future Iris backends will use them. Iris JavaScript targets ES2022 and Node.js
22 or newer.

## Values and calls

### Data, newtypes, and primitives

- A nullary data constructor, including a singleton nullary constructor such as
  `Proxy`, is a **string containing its constructor name**, not a JavaScript
  constructor instance. A fully applied constructor with arguments is a plain
  object with a string `tag` and positional `_1`, `_2`, … properties. Unapplied
  and partially applied constructors are curried functions that eventually
  produce that object. Pattern matches compare a nullary string or an object's
  `tag`, then read positional properties. Check both the producer and all
  consumers before judging a representation difference.
- A newtype has **no runtime wrapper**: direct construction and matching use
  the underlying value, and a first-class newtype constructor acts as identity.
  Erasure does not imply that arbitrary foreign values satisfy its source type.
- Records are plain objects and arrays are JavaScript arrays. Record selection
  reads a property; an update creates a new object using spread and replacement
  properties (also for nested updates). A field named `__proto__` is emitted as
  a computed property rather than a prototype-setting object-literal key.
  Spread copies only own enumerable properties: FFI-produced objects with
  inherited or non-enumerable fields need separate scrutiny.
- `Int` literals and recognized arithmetic use JavaScript numbers coerced to
  32-bit integers with bitwise operations. `Number` uses JavaScript numbers;
  `String` and `Char` use JavaScript strings; `Boolean` uses JavaScript booleans.
  Do not read a `| 0` as a different source-level numeric type.

### Functions, dictionaries, and effects

- Ordinary PureScript functions and applications are curried; output commonly
  contains one call or arrow per argument. Recognized `Data.Function.Uncurried`
  `mkFn`/`runFn` uses can instead yield multi-argument JavaScript calls/functions.
  Selected standard-library applications (including integer, number and boolean
  operations, composition, identity, and coercions) can disappear into direct
  expressions. Do not expect one emitted call per source-level call.
- Class instance dictionaries are objects with member properties; superclass
  entries are zero-argument functions returning dictionaries. Class member
  selectors read a property from a dictionary. Synthesized `IsSymbol` and
  `Reflectable` evidence supplies `reflectSymbol` and `reflectType` functions;
  trivial evidence is an empty object. Repeated closed dictionaries or member
  selections can be shared in generated module-level values, moving their
  construction or property read to module initialization. Other evidence may
  be shared locally or left inline. Even a simple single-use `let` property
  read can move into a lambda. Compare *when* a field is read, not merely how
  many textual reads appear in each file.
- Effects are zero-argument action functions. For recognized canonical `Effect`
  or `Control.Monad.ST.Internal` instance operations, generated output may
  combine pure, bind, `discardUnit`-based discard, map, or apply without calling
  each combinator. Operands may be captured or evaluated when the action is
  constructed; the action body runs when the thunk is invoked. Inspect both
  moments before calling a difference in evaluation order harmless. Similar
  user-defined monads are not automatically recognized.

## Modules, control flow, and integrations

- Each PureScript module is emitted as an ES module at `<Module>/index.js`.
  References to other modules become imports, and exports may be forwarded.
  Foreign declarations read exports from adjacent `foreign.js` or `foreign.jsx`.
  Export-name validation does not validate a foreign value's runtime shape or
  its claimed PureScript type.
- Top-level functions can appear apart from ordered top-level values. A value
  initializer may be a direct expression or an immediately invoked function.
  When all cyclic top-level initializers are instance dictionaries, generated
  code can use memoizing lazy bindings from `runtime.js`; recursive local
  non-function values can use these too. Other cyclic top-level initializers
  cause a diagnostic and emit an initializer that throws when the module loads.
  Not every module imports `runtime.js`.
- Cases and guards become branches and pattern tests; non-exhaustive execution
  can throw a pattern-match error. Tail-recursive functions may use loops or
  state dispatch rather than recursive calls. Output may materialize intermediate
  values to preserve evaluation order even when neighboring expressions inline.
- Recognized `Iris.StyleX` operations can import `@stylexjs/stylex` and call its
  APIs directly. The virtual Iris StyleX modules are not ordinary runtime
  imports. Inspect the actual output and dependencies when reviewing this path.

## Reviewing purs against Iris

These representations are **not interchangeable across generated trees**. A
different `Proxy` shape or a different point of reading a Bind field, as in the
[Control.Bind investigation in issue #559](https://github.com/purefunctor/purescript-iris/issues/559),
is neither by itself a demonstrated compiler bug nor a blanket exemption. Keep
each compiler's output, foreign files, and runtime helpers internally consistent.

1. Pin both compiler versions, dependency set, source and FFI revisions, build
   options, and entry point. Hash both generated files for every corresponding
   module pair, plus relevant foreign files and runtime helpers. Trace callers,
   imports, producers, and consumers, not just corresponding declarations.
   Re-review when inputs, hashes, or relevant guidance change. If a project
   maintains accepted known differences, check an entry's pins and hashes;
   never treat it as an exemption for new output.
2. Classify each pair as **reviewed** (with scope and evidence), **inconclusive**
   (with missing evidence), or **unreviewed**. Explain what differs in shape,
   identity, evaluation timing, and valid-program observability. Foreign code,
   unsafe coercion, cross-tree mixing, and mutable/impure dictionaries require
   separate treatment rather than an unqualified equivalence claim.
3. Before claiming a semantic mismatch, independently reason out a
   discriminating **valid PureScript** witness and the expected observation for
   each compiler. Execute it with each compiler's own consistent output and FFI;
   preserve both generated trees, observations, and failure logs. Matching
   observations cover only the exercised cases, not equivalence. Do not silently
   accept or update goldens to settle a differential finding.

## Where to check these claims

Review **generated output first**. Representative tracked outputs include
[data constructors](../tests-integration/fixtures/compiler/1784637780_data_declarations/output/Main/index.js),
[newtypes](../tests-integration/fixtures/compiler/1787506320_direct_newtype_constructor_application/output/Main/index.js),
[instance dictionaries](../tests-integration/fixtures/compiler/1787392080_cyclic_instance_dictionaries/output/Main/index.js),
[record updates](../tests-integration/fixtures/compiler/1784906340_record_update_expressions/output/Main/index.js),
[Effect actions](../tests-integration/fixtures/compiler/1787673600_effect_do_thunks/output/Main/index.js), and
[StyleX integration](../tests-integration/fixtures/compiler/1787994720_stylex_intrinsics/output/Main/index.js).
These are examples, not exhaustive contracts; regenerate output for the exact
inputs under review.

For implementation context when output is surprising, consult
[constructor/literal emission](../compiler-backend/javascript/src/convert/generator/functional/render/syntax.rs),
[newtype lowering](../compiler-backend/functional/src/convert/expression.rs),
[dictionary creation](../compiler-backend/functional/src/convert/declaration.rs),
[evidence sharing](../compiler-backend/functional/src/convert/evidence.rs),
[let inlining](../compiler-backend/functional/src/optimize.rs),
[call recognition](../compiler-backend/functional/src/convert/application.rs),
[initializers, patterns, effects and exports](../compiler-backend/javascript/src/convert/generator/functional/render.rs),
[cycle selection](../compiler-backend/javascript/src/convert/generator/functional/render/structure.rs),
[record emission](../compiler-backend/javascript/src/tree.rs),
[StyleX lowering](../compiler-backend/functional/src/convert/stylex.rs), and
[foreign validation](../compiler-backend/foreign-javascript/src/lib.rs).
For fixtures and golden regeneration, see [CONTRIBUTING.md](../CONTRIBUTING.md).

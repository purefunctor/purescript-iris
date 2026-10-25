# JavaScript output: runtime representations and differential reviews

This guide describes the JavaScript that Iris emits. It does not describe the compiler's internal
architecture or a stable cross-compiler ABI. Use it to know what to look for in
`output/<Module>/index.js` when you review output against purs. The shapes below are current Iris
conventions. purs can produce different shapes, and future Iris backends need not use these ones.
Iris JavaScript targets ES2022 and Node.js 22 or newer.

## Values and calls

### Runtime representations

| PureScript value                                            | JavaScript value                                                         |
| ----------------------------------------------------------- | ------------------------------------------------------------------------ |
| Nullary data constructor, including singletons like `Proxy` | String that contains the constructor name                                |
| Fully applied constructor with arguments                    | Plain object with a string `tag` and positional `_1`, `_2`, … properties |
| Unapplied or partially applied constructor                  | Curried function that returns that object                                |
| Newtype                                                     | The underlying value, with no wrapper                                    |
| First-class newtype constructor                             | Identity function                                                        |
| Record                                                      | Plain object                                                             |
| `Array`                                                     | JavaScript array                                                         |
| `Int`                                                       | JavaScript number, coerced to 32 bits with `\| 0`                        |
| `Number`                                                    | JavaScript number                                                        |
| `String` and `Char`                                         | JavaScript string                                                        |
| `Boolean`                                                   | JavaScript boolean                                                       |

- Pattern matches compare a nullary string or an object's `tag`, then read the positional
  properties. Check the producer and every consumer before you judge a representation difference.
- Newtype erasure does not mean that an arbitrary foreign value satisfies the newtype's source type.
- Record selection reads a property. A record update, including a nested update, creates a new
  object from a spread and replacement properties. Iris emits a field named `__proto__` as a
  computed property, so the field does not set the prototype. Spread copies only own enumerable
  properties, so review FFI objects with inherited or non-enumerable fields separately.
- `| 0` marks `Int` literals and `Int` arithmetic. It does not mean a different source-level numeric
  type.

### Inlined primitive operations

Iris emits a JavaScript operator instead of a call when a Prelude operation is applied directly to
all of its arguments and the canonical Prelude instance. A partial application, an operator bound
with `let` or passed through `flip`, and a generic or custom dictionary still call the Prelude
function or class member.

| PureScript          | Operand types                                | JavaScript                            |
| ------------------- | -------------------------------------------- | ------------------------------------- |
| `not`, `&&`, `\|\|` | `Boolean`                                    | `!`, `&&`, `\|\|`                     |
| `+`, `-`, `*`       | `Int`                                        | `left + right \| 0`, and so on        |
| `negate`            | `Int`                                        | `-value \| 0`                         |
| `negate`            | `Number`                                     | `-value`                              |
| `+`, `-`, `*`, `/`  | `Number`                                     | `+`, `-`, `*`, `/`                    |
| `<>`                | `String`                                     | `+`                                   |
| `==`, `/=`          | `Boolean`, `Int`, `Number`, `Char`, `String` | `===`, `!==`                          |
| `<`, `<=`           | `Boolean`, `Int`, `Number`, `Char`, `String` | `<`, `<=`                             |
| `>`, `>=`           | `Boolean`, `Int`, `Char`, `String`           | `>`, `>=`                             |
| `>`, `>=`           | `Number`                                     | `!(left <= right)`, `!(left < right)` |

- `&&` and `||` evaluate the right operand only when the left operand does not decide the result. A
  call to `conj` or `disj` that Iris does not inline evaluates both operands. purs inlines these
  operators the same way.
- Each comparison and arithmetic operator returns the same result as the Prelude function or class
  member it replaces. Prelude's `Number` `compare` returns `GT` when either operand is `NaN`, so
  `greaterThan` and `greaterThanOrEq` return `true`. The negated forms for `Number` `>` and `>=`
  keep that result. purs and purescript-backend-optimizer emit `>` and `>=`, which return `false`.
- `Number` `negate` is the exception. It matches purs and purescript-backend-optimizer instead of
  Prelude's definition, `zero - value`. Negating positive zero gives negative zero, and negating
  negative zero gives positive zero, also when Iris folds a literal. Source `-0.0` resolves through
  `negate` and evaluates to negative zero, so `1.0 / (-0.0)` is `-Infinity`
  ([issue #614](https://github.com/purefunctor/purescript-iris/issues/614)). Prelude's `0.0 - 0.0`
  is positive zero. Generic or custom `Ring` dictionaries keep their own `negate` behavior.

### Functions, dictionaries, and effects

- PureScript functions and applications are curried, so output usually has one call or arrow per
  argument. Recognized `Data.Function.Uncurried` `mkFn` and `runFn` uses emit multi-argument
  functions and calls instead. Some standard-library applications become direct expressions: the
  operations above, composition, identity, and coercions. Do not expect one emitted call per
  source-level call.
- A class instance dictionary is an object with one property per member. A superclass entry is a
  zero-argument function that returns the superclass dictionary. A class member selector reads a
  property from a dictionary. Synthesized `IsSymbol` and `Reflectable` evidence has `reflectSymbol`
  and `reflectType` functions. Trivial evidence is an empty object.
- Iris substitutes a `let` binding into its single use only when the use runs at most as often as
  the binding. A use inside a lambda, or inside an `Effect` action that is constructed rather than
  run in place, keeps the binding, so a returned closure or action does not repeat the work on each
  call. Literals, constructors, locals, evidence, and non-recursive globals are substituted
  everywhere. A single-use lambda is substituted even into a lambda, as purescript-backend-optimizer
  does, so composition such as `f >>> g` stays a direct expression.
- Iris can share a repeated closed dictionary or member selection as a generated module-level value.
  This moves its construction or property read to module initialization. Iris shares other evidence
  locally or leaves it inline. Compare when each field is read, not how many reads appear in each
  file.
- An effect is a zero-argument action function. For recognized canonical `Effect` and
  `Control.Monad.ST.Internal` instances, Iris can combine `pure`, `bind`, `discardUnit`-based
  discard, `map`, and `apply` without calling each combinator. Iris can capture or evaluate operands
  when it builds the action, and runs the action body when the thunk is called. Inspect both moments
  before you call an evaluation-order difference harmless. Iris does not recognize similar
  user-defined monads.

## Modules, control flow, and integrations

- Iris emits each PureScript module as an ES module at `<Module>/index.js`. References to other
  modules become imports, and exports can be forwarded. Foreign declarations read exports from the
  adjacent `foreign.js` or `foreign.jsx`. Iris validates export names, not a foreign value's runtime
  shape or its declared PureScript type.
- Top-level functions can appear apart from the ordered top-level values. A value initializer is a
  direct expression or an immediately invoked function.
- If every initializer in a top-level cycle is an instance dictionary, Iris uses memoizing lazy
  bindings from `runtime.js`. Recursive local non-function values use them too. Any other top-level
  cycle produces a diagnostic and an initializer that throws when the module loads. Only modules
  that need `runtime.js` import it.
- Cases and guards become branches and pattern tests. A non-exhaustive match can throw a
  pattern-match error at run time. Tail-recursive functions can become loops or state dispatch. Iris
  can bind intermediate values to keep evaluation order, even when neighboring expressions are
  inlined.
- Recognized `Iris.StyleX` operations can import `@stylexjs/stylex` and call its APIs directly. The
  virtual Iris StyleX modules are not runtime imports. Inspect the actual output and dependencies
  when you review this path.

## Reviewing purs against Iris

The two compilers' representations do not mix. A different `Proxy` shape, or a different point at
which a `Bind` field is read, as in the
[Control.Bind investigation in issue #559](https://github.com/purefunctor/purescript-iris/issues/559),
does not by itself show a compiler bug, and it is not a blanket exemption. Keep each compiler's
output, foreign files, and runtime helpers together.

1. Pin both compiler versions, the dependency set, the source and FFI revisions, the build options,
   and the entry point. Hash both generated files for each corresponding module pair, and the
   relevant foreign files and runtime helpers. Trace callers, imports, producers, and consumers, not
   only the corresponding declarations. Review again when inputs, hashes, or relevant guidance
   change. If a project keeps a list of accepted differences, check each entry's pins and hashes. An
   entry never exempts new output.
2. Classify each module pair and explain how the pair differs in shape, identity, evaluation timing,
   and what a valid program can observe. Foreign code, unsafe coercion, mixed trees, and mutable or
   impure dictionaries need separate treatment, not a plain equivalence claim.

   | Classification | Record with it         |
   | -------------- | ---------------------- |
   | Reviewed       | The scope and evidence |
   | Inconclusive   | The missing evidence   |
   | Unreviewed     | Nothing further        |

3. Before you claim a semantic mismatch, work out a valid PureScript program that tells the
   compilers apart, and the result you expect from each. Run it with each compiler's own output and
   FFI. Keep both generated trees, the results, and any failure logs. Matching results cover only
   the cases you ran. They do not prove equivalence. Do not accept or update goldens to settle a
   difference.

## Where to check these claims

Review generated output first. These tracked outputs are examples, not complete contracts.
Regenerate output for the exact inputs under review.

| Behavior                         | Example output                                                                                                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Data constructors                | [1784637780_data_declarations](../tests-integration/fixtures/compiler/1784637780_data_declarations/output/Main/index.js)                                           |
| Newtypes                         | [1787506320_direct_newtype_constructor_application](../tests-integration/fixtures/compiler/1787506320_direct_newtype_constructor_application/output/Main/index.js) |
| Instance dictionaries            | [1787392080_cyclic_instance_dictionaries](../tests-integration/fixtures/compiler/1787392080_cyclic_instance_dictionaries/output/Main/index.js)                     |
| Record updates                   | [1784906340_record_update_expressions](../tests-integration/fixtures/compiler/1784906340_record_update_expressions/output/Main/index.js)                           |
| Boolean and `Int` operations     | [1787547540_known_primitive_operations](../tests-integration/fixtures/compiler/1787547540_known_primitive_operations/output/Main/index.js)                         |
| Comparisons                      | [1792887683_known_primitive_comparisons](../tests-integration/fixtures/compiler/1792887683_known_primitive_comparisons/output/Main/index.js)                       |
| `Number` and `String` operations | [1792887743_known_number_and_string_operations](../tests-integration/fixtures/compiler/1792887743_known_number_and_string_operations/output/Main/index.js)         |
| Effect actions                   | [1787673600_effect_do_thunks](../tests-integration/fixtures/compiler/1787673600_effect_do_thunks/output/Main/index.js)                                             |
| StyleX integration               | [1787994720_stylex_intrinsics](../tests-integration/fixtures/compiler/1787994720_stylex_intrinsics/output/Main/index.js)                                           |

When output is surprising, read the implementation.

| Topic                                        | Source                                                                                                     |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Constructor, literal, and operator emission  | [render/syntax.rs](../compiler-backend/javascript/src/convert/generator/functional/render/syntax.rs)       |
| Newtype lowering                             | [convert/expression.rs](../compiler-backend/functional/src/convert/expression.rs)                          |
| Dictionary creation                          | [convert/declaration.rs](../compiler-backend/functional/src/convert/declaration.rs)                        |
| Evidence sharing                             | [convert/evidence.rs](../compiler-backend/functional/src/convert/evidence.rs)                              |
| Let inlining and literal folding             | [optimize.rs](../compiler-backend/functional/src/optimize.rs)                                              |
| Call and primitive operation recognition     | [convert/application.rs](../compiler-backend/functional/src/convert/application.rs)                        |
| Initializers, patterns, effects, and exports | [render.rs](../compiler-backend/javascript/src/convert/generator/functional/render.rs)                     |
| Cycle selection                              | [render/structure.rs](../compiler-backend/javascript/src/convert/generator/functional/render/structure.rs) |
| Record emission                              | [tree.rs](../compiler-backend/javascript/src/tree.rs)                                                      |
| StyleX lowering                              | [convert/stylex.rs](../compiler-backend/functional/src/convert/stylex.rs)                                  |
| Foreign validation                           | [foreign-javascript/src/lib.rs](../compiler-backend/foreign-javascript/src/lib.rs)                         |

For fixtures and golden regeneration, see [CONTRIBUTING.md](../CONTRIBUTING.md).

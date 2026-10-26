---
name: verifying-lexer
description:
  "Model-checks the lexer (compiler-frontend/lexing/src/lexer.rs) with TLA+/TLC over every source
  string up to a bounded length, drawn from small alphabets that reach each branch of the lexer. Use
  when changing Lexer, take_token_impl, any take_* method, operator_kind, the LexerCategories
  predicates, or lexing::lex, or when tokens swallow code, lose text, report wrong positions, loop
  forever, or panic inside lexing::lex."
---

# Verifying the lexer

`lexing::lex` turns source text into `Lexed`: one `SyntaxKind` per token and a `SyntaxKindInfo` with
the end offsets of the token's annotation, qualifier, and text, and the token's position. The parser
rebuilds the lossless syntax tree from these offsets, and the layout pass reads the positions to
decide where blocks start, continue, and end. The lexer runs on every keystroke in the LSP. With
wrong offsets, text disappears from the tree or comments turn into code. With wrong positions,
layout inserts separators in the wrong places. A panic or an endless loop stops the analysis.

The TLA+ model in `compiler-frontend/lexing/verification/Lexer.tla` transcribes `Lexer` method by
method. TLC lexes every source string up to a bounded length over a small alphabet and checks each
result against properties that do not reuse the lexer's code.

## Properties

| Property                | Statement                                                                                                                                                  | Harm when broken                                                                             |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `NeverPanics`           | `lexing::lex` never reaches the `unreachable!()` in `take_string` or `take_raw_string`, or the `assert_eq!` in `take_hole`.                                | `lexing::lex` crashes the LSP or the compiler on some source file.                           |
| `LexTerminates`         | Every `take_token` call consumes at least one character.                                                                                                   | The `while !lexer.is_eof()` loop in `lexing::lex` never ends.                                |
| `TokensCoverSource`     | Each token's annotation, qualifier, and text end at or after the previous end, exactly one `END_OF_FILE` comes last, and it ends at the end of the source. | `Lexed::annotation`, `qualifier`, and `text` slice out of order, and source text is lost.    |
| `AnnotationsAreTrivia`  | Each token's annotation is the longest run of whitespace, `--` line comments, and nested `{- -}` block comments after the previous token.                  | Comments become tokens, or code becomes a comment.                                           |
| `PositionsMatchOffsets` | Each token's position is the line and column of its annotation end, which is where its qualifier, or its text when it has none, starts.                    | Layout compares the wrong columns and inserts separators and block ends in the wrong places. |

`AnnotationsAreTrivia` and `PositionsMatchOffsets` compute their expected values from the source
with `TriviaEnd` and `PositionAt`, which are written independently of the model of `Lexer`.

The properties are about well-formed output. Apart from the comment rules in `TriviaEnd`, the model
does not compare token kinds with `purs`. The snapshot tests in
`compiler-frontend/lexing/tests/lexer/` and the package-set parsing check in `tests-compatibility`
cover that.

## Check a change

From the repository root, run:

```bash
compiler-frontend/lexing/verification/check-lexer.sh
```

The script uses `.agents/scripts/tlc.sh` to install Temurin 21.0.12 through mise and download
SHA-256-checked `tla2tools.jar` 1.7.4 to `target/tla/`. The run takes about 2 minutes on 16 cores.
It prints one `pass` line for each scenario and one `caught` line for each mutant, then ends with
`All results as expected` and exits with status 0. Until the [known findings](#known-findings) are
fixed, the `names` and `all` scenarios print `FAIL` and the script exits with status 1.

- `pass` means that no source string in that scenario breaks a property.
- `caught` means that a mutant of the model fails the property that guards it.
- `MISSED` means that the check no longer detects that bug. Treat it as seriously as a failing
  scenario.

The other modes are `check-lexer.sh baseline` and `check-lexer.sh mutants`. TLC writes its logs,
including counterexample traces, to `target/tla/runs/lexing/<run>.log`. The last state of a trace
holds the shortest failing `source`, because TLC builds sources one character at a time in
breadth-first order.

### Scenarios

Each scenario is an alphabet and a maximum source length. `scenarios` in `check-lexer.sh` sets the
lengths, and `Alphabets` in `Lexer.tla` sets the characters.

| Scenario   | Alphabet                        | Length | Sources   | Reaches                                                                  |
| ---------- | ------------------------------- | ------ | --------- | ------------------------------------------------------------------------ |
| `comments` | `{ - } \n a`                    | 9      | 2,441,406 | Nested and adjacent block comments, line comments, and `{`, `}`, `-`.    |
| `strings`  | `" ' \ space \n a`              | 8      | 2,015,539 | Characters, strings, raw strings, escapes, and string gaps.              |
| `numbers`  | `0 1 . x e E + - _ a`           | 6      | 1,111,111 | Integers, hexadecimals, fractions, exponents, ranges, and leading zeros. |
| `names`    | `A a . ( ) + - _ ' ? space`     | 6      | 1,948,717 | Qualifiers, operator names, holes, `_`, and operators after `(`.         |
| `all`      | Every character in `Characters` | 4      | 245,411   | Every branch of `take_token_impl` next to every other.                   |

### Known findings

At the sync commit, the `names` and `all` scenarios fail on two bugs in the real code. Until they
are fixed, these two failures are expected, and any other failure is new.

- **`AnnotationsAreTrivia` on `(--`.** `take_operator_name_or_left_parenthesis` lexes a `--` after
  `(` as an operator, so `foo = (-- the answer` yields `OPERATOR "--"`, `LOWER "the"`, and
  `LOWER "answer"`. `purs` lexes `(` and then a line comment, because its `leftParen` falls back to
  `TokLeftParen` when the operator characters are not followed by `)`. `(--)` is an operator name in
  both.
- **`PositionsMatchOffsets` on `A.(`.** `take_operator_name_or_left_parenthesis` takes the position
  of `(` from `current_position`, after the qualifier, instead of `qualifier_position`, so a
  qualified `(` or operator name such as `Prelude.(+)` is placed after its qualifier. In
  `main = do\n  foo\n  Prelude.(+) 1 2`, layout then emits no `LAYOUT_SEPARATOR` before
  `Prelude.(+)`, and the third line continues the call to `foo`.

TLC stops at the first violation, so the second finding hides behind the first in `names` and `all`.
With both fixed in a scratch copy of the model, every scenario passed. When you fix one, remove it
from this list.

## Decide whether the change touches the verified code

Compare the covered files with the sync commit:

```bash
git diff 857c44934377037ff4804dc49925df07e9a5e49d -- \
  compiler-frontend/lexing/src/lexer.rs compiler-frontend/lexing/src/categories.rs \
  compiler-frontend/lexing/src/lexed.rs compiler-frontend/lexing/src/lib.rs
```

The model is a hand-written copy of the code. After any change to `Lexer`, `operator_kind`,
`LexerCategories`, `LexedBuilder::push`, or `lexing::lex`, a pass says nothing about the new code
until `Lexer.tla` has the matching change.

## Coverage record

Sync commit: `857c44934377037ff4804dc49925df07e9a5e49d`. At this commit, the model describes the
code below. Move the sync commit forward only after you confirm that the model still describes the
code at the new commit.

| Code                                                                                    | In `Lexer.tla`                                                                                                                               |
| --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `Lexer` fields                                                                          | The `lexer` record: `source`, `index`, `position`, `annotation`, `qualifier`, `qualifierPosition`, `tokens`, and `fault`.                    |
| `first`, `second`, `is_eof`, `consumed`, `take`, `take_while`, `take_while_maximum`     | `First`, `Second`, `IsEof`, `Consumed`, `Take`, `TakeWhile`, `TakeWhileMaximum`                                                              |
| `take_annotation` and its `take_annotation_*` helpers                                   | `TakeAnnotation`, `AnnotationLoop`, `TakeAnnotationWhitespace`, `TakeAnnotationLineComment`, `TakeAnnotationBlockComment` and `BlockComment` |
| `take_qualifier` and its checkpoint restore                                             | `TakeQualifier`, `QualifierLoop`                                                                                                             |
| `push`, `LexedBuilder::push`                                                            | `Push`, `PushInfo`                                                                                                                           |
| `take_token_impl`                                                                       | `TakeTokenImpl`, with the same order of cases.                                                                                               |
| `take_kind`, `take_operator`, `take_operator_name_or_left_parenthesis`, `operator_kind` | `TakeKind`, `TakeOperator`, `TakeOperatorNameOrLeftParenthesis`, `OperatorKind`                                                              |
| `take_char`, `take_string`, `take_normal_string`, `take_raw_string`                     | `TakeChar` and `CharBody`, `TakeString`, `TakeNormalString` and `NormalStringBody`, `TakeRawString`                                          |
| `take_hole`, `take_lower`, `take_upper`, `take_integer_or_number`                       | `TakeHole`, `TakeLower`, `TakeUpper`, `TakeIntegerOrNumber`                                                                                  |
| `Lexer::new`, `Lexer::take_token`, `Lexer::finish`, `lexing::lex`                       | `New`, `TakeToken`, `Finish`, `Lex` and `LexLoop`                                                                                            |
| `unreachable!()` and `assert_eq!` in the lexer                                          | `Fail`, which records a `fault` named in `Panics`.                                                                                           |

At the sync commit, a throwaway Rust test ran `lexing::lex` on every source of every scenario at a
shorter length (comments 7, strings 6, numbers 5, names 5, all 3: 453,065 sources). It compared each
token's kind, annotation, qualifier, and token offsets, position, and error with TLC's state dump.
All 453,065 sources agreed. A model with the `qualifier_keeps_position` mutant differed on 40,941 of
the 177,156 `names` sources, which shows that the comparison can fail.

### Abstractions and bounds

- **Characters stand for their classes.** The lexer reads characters only through equality and the
  `LexerCategories` and `char` predicates. `Characters` holds one or a few representatives of each
  class: `a e x _` for lower starts, `A E` for upper starts, `0 1` for digits, `- + . ? \` for
  operators, and `x`, `e`, `E`, `'`, `"`, and the brackets for the characters that branches compare
  with. `Whitespace`, `LowerStart`, `UpperStart`, `Digit`, `HexDigit`, `Name`, and
  `OperatorCharacter` replace the predicates.
- **Every character is one byte and one column.** The model does not cover non-ASCII characters,
  `\r`, tabs, or `\0`, which `first` cannot tell apart from the end of the source.
- **Keywords are out of reach.** No keyword can be spelled from the alphabet, so `TakeLower` always
  pushes `LOWER`, and `lower_kind` is not modelled. `OperatorKind` covers only the operators that
  the alphabet can spell.
- **The source length is bounded per scenario.** A pass covers every source of that length or less
  over that alphabet, not longer sources or other character mixes. The `all` scenario mixes every
  branch, but only up to 4 characters.
- **The model covers only the lexer.** It does not cover `Lexed` accessors, layout, or the parser.

### Mutants

The `Mutant` constant in `Lexer.tla` selects a mutant. `mutants` in `check-lexer.sh` lists each
mutant with the scenario, the length, and the property that must catch it.

| Mutant                                 | Code change it stands for                                                                                          | Caught by               |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------ | ----------------------- |
| `block_comment_takes_after_delimiter`  | `take_annotation_block_comment` takes one more character after `{-` and `-}`, as it did before commit `068d35be9`. | `AnnotationsAreTrivia`  |
| `underscore_takes_nothing`             | `_` pushes `UNDERSCORE` without `take`.                                                                            | `LexTerminates`         |
| `operator_annotation_from_parenthesis` | The operator after `(` reuses the parenthesis's annotation instead of starting at the parenthesis's end.           | `TokensCoverSource`     |
| `qualifier_keeps_position`             | `take_qualifier` restores its checkpoint without restoring `current_position`.                                     | `PositionsMatchOffsets` |
| `raw_string_opens_with_four_quotes`    | `take_string` sends five quotes to `unreachable!()` instead of `take_raw_string`.                                  | `NeverPanics`           |

## When the check fails

1. Open the log named in the output and read the `source` in the last state. Characters are written
   as TLA+ strings, so `"\""` is `"` and `"\\"` is `\`.
2. Lex that source with the real code, for example as a fixture in
   `compiler-frontend/lexing/tests/lexer/`. If the real output breaks the property, the failure is a
   real bug. Report it as the source text and the property it breaks. Do not weaken the property or
   the model to make the check pass.
3. If the real output does not break the property, the model is wrong. Compare the model's tokens in
   the trace with the real tokens, find the first difference, fix the matching operator, and keep
   every mutant caught.
4. A `MISSED` mutant after a model change means that the model no longer reaches that path. Fix the
   model, not the `mutants` list in `check-lexer.sh`.

## When the change is intended

1. Edit the matching operator in `Lexer.tla` so that it describes the new code. Keep one operator
   per Rust method and the order of cases within each method.
2. If the change compares with a character that no alphabet contains, add the character to
   `Characters` and to the alphabet of the scenario that exercises it. Check the run time: each
   added character multiplies the sources by the scenario length.
3. If the change adds a branch that pushes a token, advances the lexer, or can panic, add a mutant
   that breaks it. Add it to `Mutants` in the model, to `mutants` in `check-lexer.sh`, and to the
   mutant table above.
4. Run `check-lexer.sh` until every scenario passes and every mutant is caught.
5. Update the coverage record, and move the sync commit to the commit that contains the code change.

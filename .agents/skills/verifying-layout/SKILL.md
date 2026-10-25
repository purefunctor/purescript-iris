---
name: verifying-layout
description:
  "Model-checks the layout algorithm (compiler-frontend/lexing/src/layout.rs) with TLA+/TLC over
  every token kind and every relative position, up to a bounded delimiter stack. Use when changing
  Layout, Insert, Delimiter, Collapse, or the LOWER and OPERATOR token sets in layout.rs, when
  adding token kinds to SyntaxKind, or when the parser sees unbalanced LAYOUT_START and LAYOUT_END
  tokens, lost or duplicated source tokens, or a panic inside lexing::layout."
---

# Verifying the layout algorithm

`lexing::layout` inserts `LAYOUT_START`, `LAYOUT_SEPARATOR`, and `LAYOUT_END` into the lexed token
stream. The parser uses these tokens to delimit `where`, `let`, `do`, `ado`, and `of` blocks, and it
pairs every non-layout token in the output with the next `Lexed` index. The layout pass runs on
every file the LSP sees, including half-typed and malformed files. With unbalanced blocks, the
parser builds the wrong tree. A lost or extra source token breaks the pairing with `Lexed` and the
lossless syntax tree. A panic stops the analysis.

`Layout` is a state machine. Its state is the delimiter stack `Vec<(Position, Delimiter)>`, and each
source token is one transition through `Insert::invoke`. The TLA+ model in
`compiler-frontend/lexing/verification/Layout.tla` transcribes `Insert` arm by arm. TLC explores
every reachable stack, every token kind with and without a qualifier, and every position of the next
token relative to the positions on the stack.

## Properties

| Property                 | Kind            | Statement                                                                                                                 | Harm when broken                                                                                  |
| ------------------------ | --------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `NeverPanics`            | Action property | `Layout::insert` does not panic.                                                                                          | `lexing::layout` crashes the LSP or the compiler on some source file.                             |
| `RootIsBottom`           | Invariant       | `Delimiter::Root` stays at the bottom of the stack, exactly once.                                                         | `pop_stack_if` reaches `unreachable!()`, and `is_top_declaration` stops matching.                 |
| `EndsCloseOpenBlocks`    | Action property | No `LAYOUT_END` is emitted unless a `LAYOUT_START` is open.                                                               | The parser closes a block that it never opened.                                                   |
| `SeparatorsInsideBlocks` | Action property | No `LAYOUT_SEPARATOR` is emitted unless a `LAYOUT_START` is open.                                                         | The parser sees a separator outside every block.                                                  |
| `SourceTokensPreserved`  | Action property | Each transition emits its input token exactly once, and emits only layout tokens besides it.                              | Output tokens no longer pair with `Lexed` indices, and the syntax tree loses text.                |
| `OpenBlocksMatchStack`   | Invariant       | The number of open blocks in the output equals the number of `is_indented` delimiters on the stack.                       | `finish` emits the wrong number of `LAYOUT_END`s, or a later `LAYOUT_END` closes the wrong block. |
| `FinishClosesEveryBlock` | Invariant       | `Layout::finish` on the current stack emits one `LAYOUT_END` per open block, then `END_OF_FILE`.                          | The file ends with unclosed blocks.                                                               |
| `ErasureIsSound`         | `Assert`        | The model never reads the position of a delimiter outside `PositionRead`, which the erasure abstraction below depends on. | A failure means that the model is wrong. It is not a code bug.                                    |

The action properties read the `step` variable, which records the arm, the positions, and the output
of the transition. `View` leaves `step` out of the state fingerprint, and TLC checks action
properties on every transition, including transitions into states it has already seen.

The properties are about well-formed output. The model does not compare the inserted layout with
`purs`. The snapshot tests in `compiler-frontend/lexing/tests/layout/` and the package-set parsing
check in `tests-compatibility` cover that.

## Check a change

From the repository root, run:

```bash
compiler-frontend/lexing/verification/check.sh
```

The script uses `.agents/scripts/tlc.sh` to install Temurin 21.0.12 through mise and download
SHA-256-checked `tla2tools.jar` 1.7.4 to `target/tla/`. The run takes about 30 seconds on 16 cores.
It prints one `pass` line for the baseline and one `caught` line for each mutant, then ends with
`All results as expected` and exits with status 0.

- `pass` means that no reachable state or transition of the unmutated model breaks a property. At
  the default stack depth of 4, TLC finds 145,232 distinct states.
- `caught` means that a mutant of the model fails the property that guards it, when TLC checks that
  property alone.
- `MISSED` means that the check no longer detects that bug. Treat it as seriously as a failing
  baseline.

The other modes are `check.sh baseline` and `check.sh mutants`. `LAYOUT_VERIFICATION_DEPTH` sets the
baseline stack depth, which defaults to 4. `LAYOUT_VERIFICATION_MUTANT_DEPTH` sets the mutant stack
depth, which defaults to 3. Depth 5 explores 2,721,303 distinct states and takes about 5 minutes.
Run it after you add an arm, a guard, or a `Delimiter`:

```bash
LAYOUT_VERIFICATION_DEPTH=5 compiler-frontend/lexing/verification/check.sh baseline
```

TLC writes its logs, including counterexample traces, to `target/tla/runs/lexing/<run>.log`.

## Decide whether the change touches the verified code

Compare the covered files with the sync commit:

```bash
git diff 45a17711e3ee10e3a2d4c082cee08d7a3934ee83 -- \
  compiler-frontend/lexing/src/layout.rs compiler-frontend/lexing/src/lib.rs
git diff 45a17711e3ee10e3a2d4c082cee08d7a3934ee83 -- compiler-frontend/syntax/src/lib.rs \
  | rg -B3 -A3 '^[+-]\s+[A-Z_]+,?$'
```

The model is a hand-written copy of the code. After any change to `Insert`, `Layout`, `Collapse`,
`Delimiter`, `LOWER`, or `OPERATOR`, a pass says nothing about the new code until `Layout.tla` has
the matching change. A new token kind in `SyntaxKind` that the lexer can emit belongs in
`SourceTokens`.

## Coverage record

Sync commit: `45a17711e3ee10e3a2d4c082cee08d7a3934ee83`. At this commit, the model describes the
code below. Move the sync commit forward only after you confirm that the model still describes the
code at the new commit.

| Code in `layout.rs`                                                                  | In `Layout.tla`                                                                       |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| The `match self.token` guards in `Insert::invoke`                                    | `Arm`, which maps a token kind and its qualifier to the arm that handles it.          |
| Each arm of `Insert::invoke`                                                         | `Insert`, and `InsertData`, `InsertWhere`, `InsertIn`, and so on for the larger arms. |
| `LOWER`, `OPERATOR`                                                                  | `Lower`, `Operator`                                                                   |
| `Delimiter::is_indented`                                                             | `Indented`                                                                            |
| `is_top_declaration`, `insert_default`, `insert_start`, `insert_sep`                 | `IsTopDeclaration`, `InsertDefault`, `InsertStart`, `InsertSep`                       |
| `insert_keyword_property`                                                            | `InsertKeywordProperty`, with the callback as a `LAMBDA`.                             |
| `insert_token`, `insert_separator`, `insert_end`, `push_stack`, `pop_stack`          | `InsertToken`, `InsertSeparator`, `InsertEnd`, `PushStack`, `PopStack`                |
| `pop_stack_if` and its `unreachable!()`                                              | `PopStackIf`, which records a `fault` on an empty stack.                              |
| `collapse`, `Collapse::preview`, `Collapse::commit`                                  | `Collapse` and `CollapseFrom`, `Preview` and `PreviewTopIs`, `Commit`                 |
| `indented_p`, `offside_p`, `offside_end_p`, `where_p`, `in_p`, `arrow_p`, `equals_p` | `Holds`, one case per predicate name.                                                 |
| `sep_p`, `indent_sep_p`                                                              | `SepP`, `IndentSepP`                                                                  |
| `Layout::finish`                                                                     | `Finish`, checked on every reachable stack by `FinishClosesEveryBlock`.               |
| `Layout::new`, `Layout::take_token`, `lexing::layout`                                | `Init`, and `Next` choosing the arm and the position of the next token.               |

At the sync commit, a throwaway Rust harness ran `Layout::insert` over the same abstraction, and its
results were compared with TLC's state dump. Each transition counts the arm, the positions, the
emitted tokens, and the successor state. Both sides produced the same 130,440 distinct transitions
at stack depth 3 and the same 145,232 reachable states at depth 4. A mutated model differed in
121,520 transitions, which shows that the comparison can fail.

### Abstractions and bounds

- **Token kinds are grouped by arm.** `Insert` depends on the token only through the arm of
  `Insert::invoke` that handles it, and every arm emits the token through `insert_token`. `Next`
  therefore chooses an arm from `Arms`, which `Arm` computes from every kind in `SourceTokens` with
  and without a qualifier. The output records the source token as `"Token"`.
- **Positions are relative.** `layout.rs` compares positions with `<`, `<=`, `==`, and `!=`, and
  never does arithmetic on them. After each transition, `Renumber` replaces each column with twice
  its rank among the columns in the state. `Next` offers every column equal to, between, below, or
  above them, so even columns are existing ones and odd columns fall between them.
- **Lines are earlier, current, or later.** `sep_p` is the only reader of lines, and it compares a
  stack entry's line with the current token's line. Each entry's `line` is `CurrentLine` (1) or
  `EarlierLine` (0). An entry pushed at the next token's position is `LaterLine` (2) during the
  transition when the next token starts a new line. A next token on the same line must have a
  greater column.
- **Unread positions are erased.** The code reads positions only for delimiters in `PositionRead`:
  `Indented`, `TopDecl`, and `TopDeclHead`. `Entry` stores column 0 and line 0 for every other
  delimiter. `Column` and `Line` fail an `Assert` that starts with `ErasureIsSound` if the model
  reads an erased position. If the code starts reading another delimiter's position, add that
  delimiter to `PositionRead`.
- **The stack depth is bounded.** `Next` expands only states whose stack holds at most `MaxDepth`
  entries, `Root` included. A pass covers every input of any length whose stack never grows deeper
  than the bound. Depth 4 means `Root` plus three nested delimiters, such as a `where` block, a `do`
  block, and a `case`.
- **The input is over-approximated.** Every arm can follow every other, and every token kind can be
  qualified. This includes token sequences and qualifiers that `lexing::lex` never produces. A
  counterexample that the lexer cannot produce is a robustness finding, not a user-visible bug. Say
  which it is when you report it.
- **The model covers only `layout.rs`.** It does not cover `Lexed`, the lexer, or how the parser
  consumes the output.

### Known dead code

In the `IN` arm, the `[.., (_, Delimiter::Ado), (_, Delimiter::LetStmt)]` case never matches. `in_p`
pops every indented delimiter except `Let` and `Ado`, so `LetStmt` is never on top after the
collapse. The generic indented case emits the same two `LAYOUT_END`s. Removing one `insert_end` in
the `Ado` case is therefore an equivalent mutant, and the check cannot catch it.

### Mutants

The `Mutant` constant in `Layout.tla` selects a mutant. `check.sh` lists each mutant with the
property that must catch it.

| Mutant                             | Code change it stands for                                                           | Caught by                |
| ---------------------------------- | ----------------------------------------------------------------------------------- | ------------------------ |
| `in_skip_end`                      | `IN` pops an indented block without `insert_end`.                                   | `OpenBlocksMatchStack`   |
| `start_without_token`              | `insert_start` pushes the delimiter without emitting `LAYOUT_START`.                | `OpenBlocksMatchStack`   |
| `paren_pops_unconditionally`       | `)` uses `pop_stack` instead of `pop_stack_if(Paren)`.                              | `RootIsBottom`           |
| `else_drops_token`                 | The top-level `else` case skips `insert_token`.                                     | `SourceTokensPreserved`  |
| `finish_skips_ends`                | `finish` does not emit `LAYOUT_END` for open blocks.                                | `FinishClosesEveryBlock` |
| `data_ignores_top_declaration`     | `data` pushes `TopDecl` without checking `is_top_declaration`.                      | `SeparatorsInsideBlocks` |
| `insert_start_reads_any_delimiter` | `insert_start` compares with the top delimiter instead of the nearest indented one. | `ErasureIsSound`         |
| `top_declaration_indexes_stack`    | `is_top_declaration` indexes `stack[1]` without checking the stack length.          | `NeverPanics`            |

## When the check fails

1. Open the log named in the output and read the trace from `State 1`. Each state after the first
   shows the `step` that produced it: the `arm`, the current token's `column`, the `nextColumn`,
   whether the next token is on a `newLine`, and the `output`, where `"Token"` is the source token.
   `fault` names the panic for `NeverPanics`.
2. Write the trace as a PureScript source file if you can. Columns in each state are ranks, so read
   them relative to the stack entries in the same state: an even column equals the column of an
   entry, and an odd column falls between two entries. Choose real columns that keep that order and
   those equalities, start a new line wherever `newLine` is `TRUE`, and use any text for names. Run
   it through `lexing::lex` and `lexing::layout`, for example as a fixture in
   `compiler-frontend/lexing/tests/layout/`. If the real code breaks the property on that file, the
   failure is a real bug. Report it as the source text and the property it breaks. Do not weaken the
   property or the model to make the check pass.
3. If the real code does not break the property on that file, the model is wrong. Compare the
   failing arm with `Insert::invoke` line by line, fix the model, and keep every mutant caught.
4. If the lexer cannot produce that token sequence, for example a qualified `)`, the failure is a
   robustness finding. Report it as one. Changing `SourceTokens` or the qualifier exploration to
   hide it is a decision for the user.
5. `ErasureIsSound` means that the model reads a position it erased. Add the delimiter to
   `PositionRead` if the code reads it, then rerun.
6. A `MISSED` mutant after a model change means that the model no longer reaches that path. Fix the
   model, not the `mutants` list in `check.sh`.

## When the change is intended

1. Edit the matching operator in `Layout.tla` so that it describes the new code. Keep one operator
   per Rust helper and the order of calls within each arm. For example, if `RIGHT_CURLY` stops
   checking for `Property` and `Brace`, `InsertClose` must call `PopStack` twice, and the baseline
   then fails `RootIsBottom` on a lone `}`.
2. If the change adds a guard, a new `Delimiter`, or a new arm in `Insert::invoke` that emits or
   pops layout, add a mutant that removes it. Add it to `Mutants` in the model, to `mutants` in
   `check.sh` with the property that must catch it, and to the mutant table above.
3. Run `check.sh` until the baseline passes and every mutant is caught. If you added an arm, a
   guard, or a `Delimiter`, also run the baseline at depth 5.
4. Update the coverage record, and move the sync commit to the commit that contains the code change.

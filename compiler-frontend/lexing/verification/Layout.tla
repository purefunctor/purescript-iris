------------------------------- MODULE Layout -------------------------------
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS MaxDepth, Mutant

Mutants == {
    "none",
    "in_skip_end",
    "start_without_token",
    "paren_pops_unconditionally",
    "else_drops_token",
    "finish_skips_ends",
    "data_ignores_top_declaration",
    "insert_start_reads_any_delimiter",
    "top_declaration_indexes_stack"
}

ASSUME Mutant \in Mutants
ASSUME MaxDepth \in Nat \ {0}

SourceTokens == {
    "TEXT", "HOLE", "UPPER", "LOWER", "OPERATOR", "OPERATOR_NAME",
    "AT", "BACKSLASH", "COLON", "DOUBLE_COLON", "COMMA", "EQUAL", "MINUS", "PERIOD",
    "DOUBLE_PERIOD", "DOUBLE_PERIOD_OPERATOR_NAME", "PIPE", "TICK", "UNDERSCORE",
    "LEFT_ARROW", "RIGHT_ARROW", "LEFT_THICK_ARROW", "RIGHT_THICK_ARROW",
    "LEFT_PARENTHESIS", "RIGHT_PARENTHESIS", "LEFT_CURLY", "RIGHT_CURLY",
    "LEFT_SQUARE", "RIGHT_SQUARE",
    "ADO", "AS", "CASE", "CLASS", "DATA", "DERIVE", "DO", "ELSE", "FORALL", "FOREIGN",
    "HIDING", "IF", "IMPORT", "IN", "INFIX", "INFIXL", "INFIXR", "INSTANCE", "LET",
    "MODULE", "NEWTYPE", "NOMINAL", "OF", "PHANTOM", "REPRESENTATIONAL", "ROLE", "THEN",
    "TYPE", "WHERE",
    "CHAR", "INTEGER", "NUMBER", "RAW_STRING", "STRING", "TRUE", "FALSE",
    "ERROR"
}

Lower == {
    "ADO", "AS", "CASE", "CLASS", "DATA", "DERIVE", "DO", "ELSE", "FALSE", "FORALL",
    "FOREIGN", "HIDING", "IF", "IMPORT", "IN", "INFIX", "INFIXL", "INFIXR", "INSTANCE",
    "LET", "LOWER", "MODULE", "NEWTYPE", "NOMINAL", "OF", "PHANTOM", "REPRESENTATIONAL",
    "ROLE", "THEN", "TRUE", "TYPE", "WHERE"
}

Operator == {"OPERATOR", "MINUS", "COLON", "DOUBLE_PERIOD", "LEFT_THICK_ARROW"}

Arm(token, qualified) ==
    IF token = "DATA" /\ ~qualified THEN "DATA"
    ELSE IF token = "CLASS" /\ ~qualified THEN "CLASS"
    ELSE IF token = "WHERE" /\ ~qualified THEN "WHERE"
    ELSE IF token = "IN" /\ ~qualified THEN "IN"
    ELSE IF token = "LET" /\ ~qualified THEN "LET"
    ELSE IF token = "DO" THEN "DO"
    ELSE IF token = "ADO" THEN "ADO"
    ELSE IF token = "CASE" /\ ~qualified THEN "CASE"
    ELSE IF token = "OF" /\ ~qualified THEN "OF"
    ELSE IF token = "IF" /\ ~qualified THEN "IF"
    ELSE IF token = "THEN" /\ ~qualified THEN "THEN"
    ELSE IF token = "ELSE" /\ ~qualified THEN "ELSE"
    ELSE IF token = "FORALL" THEN "FORALL"
    ELSE IF token = "BACKSLASH" THEN "BACKSLASH"
    ELSE IF token = "RIGHT_ARROW" THEN "RIGHT_ARROW"
    ELSE IF token = "EQUAL" THEN "EQUAL"
    ELSE IF token = "PIPE" THEN "PIPE"
    ELSE IF token = "TICK" THEN "TICK"
    ELSE IF token = "COMMA" THEN "COMMA"
    ELSE IF token = "PERIOD" THEN "PERIOD"
    ELSE IF token = "LEFT_PARENTHESIS" THEN "LEFT_PARENTHESIS"
    ELSE IF token = "LEFT_CURLY" THEN "LEFT_CURLY"
    ELSE IF token = "LEFT_SQUARE" THEN "LEFT_SQUARE"
    ELSE IF token = "RIGHT_PARENTHESIS" THEN "RIGHT_PARENTHESIS"
    ELSE IF token = "RIGHT_CURLY" THEN "RIGHT_CURLY"
    ELSE IF token = "RIGHT_SQUARE" THEN "RIGHT_SQUARE"
    ELSE IF token \in {"STRING", "RAW_STRING"} THEN "STRING"
    ELSE IF token \in Lower /\ ~qualified THEN "LOWER"
    ELSE IF token \in Operator THEN "OPERATOR"
    ELSE "DEFAULT"

Arms == {Arm(token, qualified) : token \in SourceTokens, qualified \in BOOLEAN}

Indented == {"Let", "LetStmt", "Where", "Of", "Do", "Ado"}

PositionRead == Indented \cup {"TopDecl", "TopDeclHead"}

CurrentLine == 1
EarlierLine == 0
LaterLine == 2

Erased == [column |-> 0, line |-> EarlierLine]

Entry(position, delimiter) ==
    IF delimiter \in PositionRead
    THEN [delimiter |-> delimiter, column |-> position.column, line |-> position.line]
    ELSE [delimiter |-> delimiter, column |-> Erased.column, line |-> Erased.line]

Column(entry) ==
    IF entry.delimiter \in PositionRead
    THEN entry.column
    ELSE Assert(FALSE, <<"ErasureIsSound: reads the erased position of", entry.delimiter>>)

Line(entry) ==
    IF entry.delimiter \in PositionRead
    THEN entry.line
    ELSE Assert(FALSE, <<"ErasureIsSound: reads the erased position of", entry.delimiter>>)

Position(step) == [column |-> step.column, line |-> CurrentLine]

NextPosition(step) ==
    [column |-> step.nextColumn, line |-> IF step.newLine THEN LaterLine ELSE CurrentLine]

----------------------------------------------------------------------------

Top(machine) == machine.stack[Len(machine.stack)]

TopIs(machine, delimiters) ==
    Len(machine.stack) > 0 /\ Top(machine).delimiter \in delimiters

Fault(machine, message) ==
    IF machine.fault = "none" THEN [machine EXCEPT !.fault = message] ELSE machine

Emit(machine, token) == [machine EXCEPT !.output = Append(@, token)]

InsertToken(machine) == Emit(machine, "Token")

InsertSeparator(machine) == Emit(machine, "Separator")

InsertEnd(machine) == Emit(machine, "End")

PushStack(machine, position, delimiter) ==
    [machine EXCEPT !.stack = Append(@, Entry(position, delimiter))]

PopStack(machine) ==
    IF Len(machine.stack) = 0
    THEN machine
    ELSE [machine EXCEPT !.stack = SubSeq(@, 1, Len(@) - 1)]

PopStackIf(machine, delimiters) ==
    IF Len(machine.stack) = 0
    THEN Fault(machine, "pop_stack_if on an empty stack reaches unreachable!()")
    ELSE IF Top(machine).delimiter \in delimiters
    THEN PopStack(machine)
    ELSE machine

----------------------------------------------------------------------------

SepP(step, entry) == step.column = Column(entry) /\ CurrentLine # Line(entry)

IndentSepP(step, entry) == entry.delimiter \in Indented /\ SepP(step, entry)

Holds(predicate, step, entry) ==
    LET indented == entry.delimiter \in Indented
        offsideEnd == indented /\ step.column <= Column(entry)
    IN CASE predicate = "indented_p" -> indented
         [] predicate = "offside_p" -> indented /\ step.column < Column(entry)
         [] predicate = "offside_end_p" -> offsideEnd
         [] predicate = "where_p" -> entry.delimiter = "Do" \/ offsideEnd
         [] predicate = "in_p" ->
                IF entry.delimiter \in {"Let", "Ado"} THEN FALSE ELSE indented
         [] predicate = "arrow_p" ->
                CASE entry.delimiter = "Do" -> TRUE
                  [] entry.delimiter = "Of" -> FALSE
                  [] OTHER -> offsideEnd
         [] predicate = "equals_p" -> entry.delimiter \in {"Where", "Let", "LetStmt"}

RECURSIVE CollapseFrom(_, _, _, _)
CollapseFrom(stack, length, predicate, step) ==
    IF length > 0 /\ Holds(predicate, step, stack[length])
    THEN LET rest == CollapseFrom(stack, length - 1, predicate, step)
         IN [length |-> rest.length,
             ends |-> rest.ends + (IF stack[length].delimiter \in Indented THEN 1 ELSE 0)]
    ELSE [length |-> length, ends |-> 0]

Collapse(machine, predicate, step) ==
    CollapseFrom(machine.stack, Len(machine.stack), predicate, step)

Preview(machine, collapse) == SubSeq(machine.stack, 1, collapse.length)

PreviewTopIs(machine, collapse, delimiters) ==
    collapse.length > 0 /\ machine.stack[collapse.length].delimiter \in delimiters

Commit(machine, collapse) ==
    [machine EXCEPT !.stack = SubSeq(@, 1, collapse.length),
                    !.output = @ \o [index \in 1..collapse.ends |-> "End"]]

CollapseAndCommit(machine, predicate, step) ==
    Commit(machine, Collapse(machine, predicate, step))

----------------------------------------------------------------------------

IsTopDeclaration(machine, step) ==
    /\ Len(machine.stack) = 2
    /\ machine.stack[1].delimiter = "Root"
    /\ machine.stack[2].delimiter = "Where"
    /\ step.column = Column(machine.stack[2])

TopDeclarationPanics(machine) ==
    Mutant = "top_declaration_indexes_stack" /\ Len(machine.stack) < 2

InsertSep(machine, step) ==
    IF Len(machine.stack) = 0
    THEN machine
    ELSE LET entry == Top(machine)
         IN CASE entry.delimiter \in {"TopDecl", "TopDeclHead"} /\ SepP(step, entry) ->
                    InsertSeparator(PopStack(machine))
              [] IndentSepP(step, entry) ->
                    IF entry.delimiter = "Of"
                    THEN PushStack(InsertSeparator(machine), Position(step), "CaseBinders")
                    ELSE InsertSeparator(machine)
              [] OTHER -> machine

InsertDefault(machine, step) ==
    InsertToken(InsertSep(CollapseAndCommit(machine, "offside_p", step), step))

NearestIndented(machine) ==
    LET indices == {index \in 1..Len(machine.stack) :
                        machine.stack[index].delimiter \in Indented}
    IN IF indices = {} THEN 0 ELSE CHOOSE index \in indices : \A other \in indices : other <= index

InsertStart(machine, step, delimiter) ==
    LET past == IF Mutant = "insert_start_reads_any_delimiter"
                THEN Len(machine.stack)
                ELSE NearestIndented(machine)
        pushed == PushStack(machine, NextPosition(step), delimiter)
    IN IF past # 0 /\ step.nextColumn <= Column(machine.stack[past])
       THEN machine
       ELSE IF Mutant = "start_without_token"
       THEN pushed
       ELSE Emit(pushed, "Start")

InsertKeywordProperty(machine, step, Callback(_)) ==
    LET inserted == InsertDefault(machine, step)
    IN IF TopIs(inserted, {"Property"}) THEN PopStack(inserted) ELSE Callback(inserted)

----------------------------------------------------------------------------

InsertData(machine, step, delimiter) ==
    LET inserted == InsertDefault(machine, step)
    IN IF TopDeclarationPanics(inserted)
       THEN Fault(inserted, "is_top_declaration indexes past the end of the stack")
       ELSE IF IsTopDeclaration(inserted, step) \/ Mutant = "data_ignores_top_declaration"
       THEN PushStack(inserted, Position(step), delimiter)
       ELSE PopStackIf(inserted, {"Property"})

InsertWhere(machine, step) ==
    IF TopIs(machine, {"TopDeclHead"})
    THEN InsertStart(InsertToken(PopStack(machine)), step, "Where")
    ELSE IF TopIs(machine, {"Property"})
    THEN InsertToken(PopStack(machine))
    ELSE InsertStart(InsertToken(CollapseAndCommit(machine, "where_p", step)), step, "Where")

InsertIn(machine, step) ==
    LET collapse == Collapse(machine, "in_p", step)
        preview == Preview(machine, collapse)
        committed == Commit(machine, collapse)
    IN IF Len(preview) >= 2 /\ preview[Len(preview) - 1].delimiter = "Ado"
                            /\ preview[Len(preview)].delimiter = "LetStmt"
       THEN InsertToken(InsertEnd(InsertEnd(PopStack(PopStack(committed)))))
       ELSE IF PreviewTopIs(machine, collapse, Indented)
       THEN IF Mutant = "in_skip_end"
            THEN InsertToken(PopStack(committed))
            ELSE InsertToken(InsertEnd(PopStack(committed)))
       ELSE PopStackIf(InsertDefault(machine, step), {"Property"})

InsertLet(machine, step) ==
    InsertKeywordProperty(machine, step, LAMBDA inserted:
        IF TopIs(inserted, {"Do", "Ado"}) /\ Column(Top(inserted)) = step.column
        THEN InsertStart(inserted, step, "LetStmt")
        ELSE InsertStart(inserted, step, "Let"))

InsertOf(machine, step) ==
    LET collapse == Collapse(machine, "indented_p", step)
    IN IF PreviewTopIs(machine, collapse, {"Case"})
       THEN LET popped == InsertToken(PopStack(Commit(machine, collapse)))
            IN PushStack(InsertStart(popped, step, "Of"), NextPosition(step), "CaseBinders")
       ELSE PopStackIf(InsertDefault(machine, step), {"Property"})

InsertThen(machine, step) ==
    LET collapse == Collapse(machine, "indented_p", step)
    IN IF PreviewTopIs(machine, collapse, {"If"})
       THEN PushStack(InsertToken(PopStack(Commit(machine, collapse))), Position(step), "Then")
       ELSE PopStackIf(InsertDefault(machine, step), {"Property"})

InsertElse(machine, step) ==
    LET collapse == Collapse(machine, "indented_p", step)
        committed == CollapseAndCommit(machine, "offside_p", step)
    IN IF PreviewTopIs(machine, collapse, {"Then"})
       THEN InsertToken(PopStack(Commit(machine, collapse)))
       ELSE IF TopDeclarationPanics(committed)
       THEN Fault(committed, "is_top_declaration indexes past the end of the stack")
       ELSE IF IsTopDeclaration(committed, step)
       THEN IF Mutant = "else_drops_token" THEN committed ELSE InsertToken(committed)
       ELSE PopStackIf(InsertToken(InsertSep(committed, step)), {"Property"})

InsertRightArrow(machine, step) ==
    LET collapsed == CollapseAndCommit(machine, "arrow_p", step)
    IN InsertToken(PopStackIf(collapsed, {"CaseBinders", "CaseGuard", "LambdaBinders"}))

InsertEqual(machine, step) ==
    LET collapse == Collapse(machine, "equals_p", step)
    IN IF PreviewTopIs(machine, collapse, {"DeclGuard"})
       THEN InsertToken(PopStack(Commit(machine, collapse)))
       ELSE InsertDefault(machine, step)

InsertPipe(machine, step) ==
    LET collapse == Collapse(machine, "offside_end_p", step)
        committed == Commit(machine, collapse)
    IN IF PreviewTopIs(machine, collapse, {"Of"})
       THEN InsertToken(PushStack(committed, Position(step), "CaseGuard"))
       ELSE IF PreviewTopIs(machine, collapse, {"Let", "LetStmt", "Where"})
       THEN InsertToken(PushStack(committed, Position(step), "DeclGuard"))
       ELSE InsertDefault(machine, step)

InsertTick(machine, step) ==
    LET collapse == Collapse(machine, "indented_p", step)
    IN IF PreviewTopIs(machine, collapse, {"Tick"})
       THEN InsertToken(PopStack(Commit(machine, collapse)))
       ELSE LET committed == CollapseAndCommit(machine, "offside_end_p", step)
            IN PushStack(InsertToken(InsertSep(committed, step)), Position(step), "Tick")

InsertComma(machine, step) ==
    LET committed == CollapseAndCommit(machine, "indented_p", step)
    IN IF TopIs(committed, {"Brace"})
       THEN PushStack(InsertToken(committed), Position(step), "Property")
       ELSE InsertToken(committed)

InsertPeriod(machine, step) ==
    LET inserted == InsertDefault(machine, step)
    IN IF TopIs(inserted, {"Forall"})
       THEN PopStack(inserted)
       ELSE PushStack(inserted, Position(step), "Property")

InsertClose(machine, step, delimiters) ==
    LET committed == CollapseAndCommit(machine, "indented_p", step)
    IN InsertToken(
         IF Mutant = "paren_pops_unconditionally" /\ delimiters = <<"Paren">>
         THEN PopStack(committed)
         ELSE IF Len(delimiters) = 1
         THEN PopStackIf(committed, {delimiters[1]})
         ELSE PopStackIf(PopStackIf(committed, {delimiters[1]}), {delimiters[2]}))

Insert(machine, step) ==
    CASE step.arm = "DATA" -> InsertData(machine, step, "TopDecl")
      [] step.arm = "CLASS" -> InsertData(machine, step, "TopDeclHead")
      [] step.arm = "WHERE" -> InsertWhere(machine, step)
      [] step.arm = "IN" -> InsertIn(machine, step)
      [] step.arm = "LET" -> InsertLet(machine, step)
      [] step.arm = "DO" ->
            InsertKeywordProperty(machine, step, LAMBDA inserted: InsertStart(inserted, step, "Do"))
      [] step.arm = "ADO" ->
            InsertKeywordProperty(machine, step, LAMBDA inserted: InsertStart(inserted, step, "Ado"))
      [] step.arm = "CASE" ->
            InsertKeywordProperty(machine, step, LAMBDA inserted:
                PushStack(inserted, Position(step), "Case"))
      [] step.arm = "OF" -> InsertOf(machine, step)
      [] step.arm = "IF" ->
            InsertKeywordProperty(machine, step, LAMBDA inserted:
                PushStack(inserted, Position(step), "If"))
      [] step.arm = "THEN" -> InsertThen(machine, step)
      [] step.arm = "ELSE" -> InsertElse(machine, step)
      [] step.arm = "FORALL" ->
            InsertKeywordProperty(machine, step, LAMBDA inserted:
                PushStack(inserted, Position(step), "Forall"))
      [] step.arm = "BACKSLASH" ->
            PushStack(InsertDefault(machine, step), Position(step), "LambdaBinders")
      [] step.arm = "RIGHT_ARROW" -> InsertRightArrow(machine, step)
      [] step.arm = "EQUAL" -> InsertEqual(machine, step)
      [] step.arm = "PIPE" -> InsertPipe(machine, step)
      [] step.arm = "TICK" -> InsertTick(machine, step)
      [] step.arm = "COMMA" -> InsertComma(machine, step)
      [] step.arm = "PERIOD" -> InsertPeriod(machine, step)
      [] step.arm = "LEFT_PARENTHESIS" ->
            PushStack(InsertDefault(machine, step), Position(step), "Paren")
      [] step.arm = "LEFT_CURLY" ->
            LET inserted == InsertDefault(machine, step)
            IN PushStack(PushStack(inserted, Position(step), "Brace"), Position(step), "Property")
      [] step.arm = "LEFT_SQUARE" ->
            PushStack(InsertDefault(machine, step), Position(step), "Square")
      [] step.arm = "RIGHT_PARENTHESIS" -> InsertClose(machine, step, <<"Paren">>)
      [] step.arm = "RIGHT_CURLY" -> InsertClose(machine, step, <<"Property", "Brace">>)
      [] step.arm = "RIGHT_SQUARE" -> InsertClose(machine, step, <<"Square">>)
      [] step.arm \in {"STRING", "LOWER"} ->
            PopStackIf(InsertDefault(machine, step), {"Property"})
      [] step.arm = "OPERATOR" ->
            InsertToken(InsertSep(CollapseAndCommit(machine, "offside_end_p", step), step))
      [] step.arm = "DEFAULT" -> InsertDefault(machine, step)

Finish(stack) ==
    LET ends == IF Mutant = "finish_skips_ends"
                THEN 0
                ELSE Cardinality({index \in 1..Len(stack) : stack[index].delimiter \in Indented})
    IN [index \in 1..ends |-> "End"] \o <<"EndOfFile">>

----------------------------------------------------------------------------

VARIABLES stack, column, open, step

vars == <<stack, column, open, step>>

View == <<stack, column, open>>

ReadColumns(entries) ==
    {entries[index].column : index \in
        {index \in 1..Len(entries) : entries[index].delimiter \in PositionRead}}

Rank(columns, value) == 2 * Cardinality({other \in columns : other <= value})

Renumber(entries, nextColumn, newLine) ==
    LET columns == ReadColumns(entries) \cup {nextColumn}
        Shift(line) ==
            IF ~newLine THEN line ELSE IF line = LaterLine THEN CurrentLine ELSE EarlierLine
    IN [index \in 1..Len(entries) |->
          IF entries[index].delimiter \in PositionRead
          THEN [entries[index] EXCEPT !.column = Rank(columns, @), !.line = Shift(@)]
          ELSE entries[index]]

Count(output, token) == Cardinality({index \in 1..Len(output) : output[index] = token})

Init ==
    /\ stack = <<Entry([column |-> 1, line |-> CurrentLine], "Root")>>
    /\ column = 2
    /\ open = 0
    /\ step = [arm |-> "none", column |-> 0, nextColumn |-> 0, newLine |-> FALSE,
               openBefore |-> 0, output |-> <<>>, fault |-> "none"]

Take(arm, nextColumn, newLine) ==
    LET taken == [arm |-> arm, column |-> column, nextColumn |-> nextColumn, newLine |-> newLine]
        machine == Insert([stack |-> stack, output |-> <<>>, fault |-> "none"], taken)
    IN /\ stack' = Renumber(machine.stack, nextColumn, newLine)
       /\ column' = Rank(ReadColumns(machine.stack) \cup {nextColumn}, nextColumn)
       /\ open' = open + Count(machine.output, "Start") - Count(machine.output, "End")
       /\ step' = [arm |-> arm, column |-> column, nextColumn |-> nextColumn,
                   newLine |-> newLine, openBefore |-> open, output |-> machine.output,
                   fault |-> machine.fault]

Next ==
    /\ Len(stack) <= MaxDepth
    /\ \E arm \in Arms, nextColumn \in 1..(2 * Cardinality(ReadColumns(stack) \cup {column}) + 1),
          newLine \in BOOLEAN :
          /\ newLine \/ nextColumn > column
          /\ Take(arm, nextColumn, newLine)

Spec == Init /\ [][Next]_vars

----------------------------------------------------------------------------

RECURSIVE OpenAfter(_, _, _)
OpenAfter(output, length, before) ==
    IF length = 0
    THEN before
    ELSE OpenAfter(output, length - 1, before)
         + (CASE output[length] = "Start" -> 1
              [] output[length] = "End" -> -1
              [] OTHER -> 0)

IndentedCount(entries) ==
    Cardinality({index \in 1..Len(entries) : entries[index].delimiter \in Indented})

NeverPanics == [][step'.fault = "none"]_vars

EndsCloseOpenBlocks ==
    [][\A length \in 1..Len(step'.output) :
          step'.output[length] = "End" => OpenAfter(step'.output, length - 1, step'.openBefore) > 0]_vars

SeparatorsInsideBlocks ==
    [][\A length \in 1..Len(step'.output) :
          step'.output[length] = "Separator"
              => OpenAfter(step'.output, length - 1, step'.openBefore) > 0]_vars

SourceTokensPreserved ==
    [][Count(step'.output, "Token") = 1
       /\ \A index \in 1..Len(step'.output) :
              step'.output[index] \in {"Start", "Separator", "End", "Token"}]_vars

RootIsBottom ==
    /\ Len(stack) >= 1
    /\ stack[1].delimiter = "Root"
    /\ \A index \in 2..Len(stack) : stack[index].delimiter # "Root"

OpenBlocksMatchStack == open = IndentedCount(stack)

FinishClosesEveryBlock ==
    LET closing == Finish(stack)
    IN Count(closing, "End") = open /\ closing[Len(closing)] = "EndOfFile"

=============================================================================

------------------------------- MODULE Lexer --------------------------------
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS Scenario, MaxLength, Mutant

Mutants == {
    "none",
    "block_comment_takes_after_delimiter",
    "underscore_takes_nothing",
    "operator_annotation_from_parenthesis",
    "qualifier_keeps_position",
    "raw_string_opens_with_four_quotes",
    "parenthesis_ignores_line_comment",
    "parenthesis_position_after_qualifier"
}

ASSUME Mutant \in Mutants
ASSUME MaxLength \in Nat

EofChar == "EOF_CHAR"

Whitespace == {" ", "\n"}
LowerStart == {"a", "e", "x", "_"}
UpperStart == {"A", "E"}
Digit == {"0", "1"}
HexDigit == {"0", "1", "a", "e", "A", "E"}
Name == LowerStart \cup UpperStart \cup Digit \cup {"'"}
OperatorCharacter == {"-", "+", ".", "?", "\\"}

Characters ==
    Whitespace \cup LowerStart \cup UpperStart \cup Digit \cup OperatorCharacter
        \cup {"(", ")", "{", "}", ",", "'", "\""}

Alphabets == [
    comments |-> {"{", "-", "}", "\n", "a"},
    strings |-> {"\"", "'", "\\", " ", "\n", "a"},
    numbers |-> {"0", "1", ".", "x", "e", "E", "+", "-", "_", "a"},
    names |-> {"A", "a", ".", "(", ")", "+", "-", "_", "'", "?", " "},
    all |-> Characters
]

ASSUME Scenario \in DOMAIN Alphabets

Alphabet == Alphabets[Scenario]

ASSUME Alphabet \subseteq Characters

VARIABLES source, lexed

vars == <<source, lexed>>

Char(lexer, index) ==
    IF index <= Len(lexer.source) THEN lexer.source[index] ELSE EofChar

First(lexer) == Char(lexer, lexer.index)
Second(lexer) == Char(lexer, lexer.index + 1)
IsEof(lexer) == lexer.index > Len(lexer.source)
Consumed(lexer) == lexer.index - 1

Text(start, end) == SubSeq(end.source, Consumed(start) + 1, Consumed(end))

Fail(lexer, fault) == [lexer EXCEPT !.fault = fault]

Take(lexer) ==
    LET character == First(lexer) IN
    [lexer EXCEPT
        !.index = IF IsEof(lexer) THEN @ ELSE @ + 1,
        !.position =
            IF character = "\n"
            THEN [line |-> @.line + 1, column |-> 1]
            ELSE [@ EXCEPT !.column = @ + 1]]

RECURSIVE TakeWhile(_, _)
TakeWhile(lexer, characters) ==
    IF First(lexer) \in characters /\ ~IsEof(lexer)
    THEN TakeWhile(Take(lexer), characters)
    ELSE lexer

RECURSIVE TakeWhileMaximum(_, _, _)
TakeWhileMaximum(lexer, characters, maximum) ==
    IF maximum > 0 /\ First(lexer) \in characters /\ ~IsEof(lexer)
    THEN TakeWhileMaximum(Take(lexer), characters, maximum - 1)
    ELSE lexer

TakeAnnotationWhitespace(lexer) == TakeWhile(lexer, Whitespace)

TakeAnnotationLineComment(lexer) == TakeWhile(Take(Take(lexer)), Characters \ {"\n"})

RECURSIVE BlockComment(_, _)
BlockComment(lexer, level) ==
    IF level > 0 /\ ~IsEof(lexer) THEN
        IF First(lexer) = "{" /\ Second(lexer) = "-" THEN
            BlockComment(Take(Take(lexer)), level + 1)
        ELSE IF First(lexer) = "-" /\ Second(lexer) = "}" THEN
            BlockComment(Take(Take(lexer)), level - 1)
        ELSE
            BlockComment(Take(lexer), level)
    ELSE lexer

RECURSIVE BlockCommentTakingAfterDelimiter(_, _)
BlockCommentTakingAfterDelimiter(lexer, level) ==
    IF ~IsEof(lexer) THEN
        LET matched ==
                IF First(lexer) = "{" /\ Second(lexer) = "-" THEN
                    [lexer |-> Take(Take(lexer)), level |-> level + 1]
                ELSE IF First(lexer) = "-" /\ Second(lexer) = "}" THEN
                    [lexer |-> Take(Take(lexer)), level |-> level - 1]
                ELSE
                    [lexer |-> lexer, level |-> level]
        IN IF matched.level = 0
           THEN matched.lexer
           ELSE BlockCommentTakingAfterDelimiter(Take(matched.lexer), matched.level)
    ELSE lexer

TakeAnnotationBlockComment(lexer) ==
    IF Mutant = "block_comment_takes_after_delimiter"
    THEN BlockCommentTakingAfterDelimiter(Take(Take(lexer)), 1)
    ELSE BlockComment(Take(Take(lexer)), 1)

RECURSIVE AnnotationLoop(_)
AnnotationLoop(lexer) ==
    IF First(lexer) \in Whitespace THEN
        AnnotationLoop(TakeAnnotationWhitespace(lexer))
    ELSE IF First(lexer) = "-" /\ Second(lexer) = "-" THEN
        AnnotationLoop(TakeAnnotationLineComment(lexer))
    ELSE IF First(lexer) = "{" /\ Second(lexer) = "-" THEN
        AnnotationLoop(TakeAnnotationBlockComment(lexer))
    ELSE lexer

TakeAnnotation(lexer) ==
    LET annotated == AnnotationLoop(lexer) IN
    [annotated EXCEPT
        !.annotation = Consumed(annotated),
        !.qualifierPosition = annotated.position]

RECURSIVE QualifierLoop(_)
QualifierLoop(lexer) ==
    IF First(lexer) \in UpperStart THEN
        LET named == TakeWhile(lexer, Name) IN
        IF First(named) = "." THEN QualifierLoop(Take(named))
        ELSE IF Mutant = "qualifier_keeps_position"
        THEN [lexer EXCEPT !.position = named.position]
        ELSE lexer
    ELSE lexer

TakeQualifier(lexer) ==
    LET qualified == QualifierLoop(lexer) IN
    [qualified EXCEPT !.qualifier = Consumed(qualified)]

PushInfo(lexer, kind, info, error) ==
    [lexer EXCEPT !.tokens = Append(@, [kind |-> kind, error |-> error] @@ info)]

Push(lexer, kind, error) ==
    PushInfo(lexer, kind,
        [annotation |-> lexer.annotation,
         qualifier |-> lexer.qualifier,
         token |-> Consumed(lexer),
         position |-> lexer.qualifierPosition],
        error)

TakeKind(lexer, kind) == Push(Take(lexer), kind, "None")

OperatorKind(text) ==
    CASE text = <<".">> -> "PERIOD"
      [] text = <<".", ".">> -> "DOUBLE_PERIOD"
      [] text = <<"-">> -> "MINUS"
      [] text = <<"\\">> -> "BACKSLASH"
      [] OTHER -> "OPERATOR"

TakeOperator(lexer) ==
    LET operator == TakeWhile(lexer, OperatorCharacter) IN
    Push(operator, OperatorKind(Text(lexer, operator)), "None")

TakeOperatorNameOrLeftParenthesis(lexer) ==
    LET parenthesis == Take(lexer)
        parenthesisPosition ==
            IF Mutant = "parenthesis_position_after_qualifier"
            THEN lexer.position
            ELSE lexer.qualifierPosition
        parenthesisInfo ==
            [annotation |-> lexer.annotation,
             qualifier |-> lexer.qualifier,
             token |-> Consumed(parenthesis),
             position |-> parenthesisPosition]
        lineComment ==
            /\ Mutant # "parenthesis_ignores_line_comment"
            /\ First(parenthesis) = "-"
            /\ Second(parenthesis) = "-"
            /\ First(TakeWhile(parenthesis, OperatorCharacter)) # ")"
    IN
    IF First(parenthesis) \notin OperatorCharacter \/ lineComment THEN
        PushInfo(parenthesis, "LEFT_PARENTHESIS", parenthesisInfo, "None")
    ELSE
        LET operator == TakeWhile(parenthesis, OperatorCharacter)
            operatorKind == OperatorKind(Text(parenthesis, operator))
        IN
        IF First(operator) # ")" THEN
            LET operatorInfo ==
                    [annotation |->
                        IF Mutant = "operator_annotation_from_parenthesis"
                        THEN parenthesisInfo.annotation
                        ELSE parenthesisInfo.token,
                     qualifier |-> parenthesisInfo.token,
                     token |-> Consumed(operator),
                     position |-> parenthesis.position]
                opened == PushInfo(operator, "LEFT_PARENTHESIS", parenthesisInfo, "None")
            IN PushInfo(opened, operatorKind, operatorInfo, "None")
        ELSE
            LET closed == Take(operator)
                closedInfo ==
                    [annotation |-> parenthesisInfo.annotation,
                     qualifier |-> parenthesisInfo.qualifier,
                     token |-> Consumed(closed),
                     position |-> parenthesisPosition]
                nameKind ==
                    IF operatorKind = "DOUBLE_PERIOD"
                    THEN "DOUBLE_PERIOD_OPERATOR_NAME"
                    ELSE "OPERATOR_NAME"
            IN PushInfo(closed, nameKind, closedInfo, "None")

RECURSIVE CharBody(_)
CharBody(lexer) ==
    IF ~IsEof(lexer) THEN
        IF First(lexer) = "'" THEN lexer
        ELSE IF First(lexer) = "\\" /\ Second(lexer) \in {"\\", "'"} THEN
            CharBody(Take(Take(lexer)))
        ELSE CharBody(Take(lexer))
    ELSE lexer

TakeChar(lexer) ==
    LET body == CharBody(Take(lexer)) IN
    IF First(body) = "'"
    THEN Push(Take(body), "CHAR", "None")
    ELSE Push(body, "CHAR", "Unterminated character literal")

RECURSIVE NormalStringBody(_)
NormalStringBody(lexer) ==
    IF ~IsEof(lexer) THEN
        IF First(lexer) = "\"" THEN lexer
        ELSE IF First(lexer) = "\\" /\ Second(lexer) \in Whitespace THEN
            LET gap == TakeWhile(Take(lexer), Whitespace) IN
            NormalStringBody(IF First(gap) = "\\" THEN Take(gap) ELSE gap)
        ELSE IF First(lexer) = "\\" /\ Second(lexer) \in {"\\", "\""} THEN
            NormalStringBody(Take(Take(lexer)))
        ELSE NormalStringBody(Take(lexer))
    ELSE lexer

TakeNormalString(lexer) ==
    LET body == NormalStringBody(lexer) IN
    IF First(body) = "\""
    THEN Push(Take(body), "STRING", "None")
    ELSE Push(body, "STRING", "Unterminated single quoted string literal")

RECURSIVE TakeRawString(_)
TakeRawString(lexer) ==
    LET text == TakeWhile(lexer, Characters \ {"\""})
        quoted == TakeWhileMaximum(text, {"\""}, 5)
        quotes == Consumed(quoted) - Consumed(text)
    IN
    CASE quotes = 0 -> Push(quoted, "RAW_STRING", "Unterminated triple quoted string literal")
      [] quotes \in 1..2 -> TakeRawString(quoted)
      [] quotes \in 3..5 -> Push(quoted, "RAW_STRING", "None")
      [] OTHER -> Fail(quoted, "take_raw_string reaches unreachable!()")

RawStringOpening ==
    IF Mutant = "raw_string_opens_with_four_quotes" THEN 3..4 ELSE 3..5

TakeString(lexer) ==
    LET quoted == TakeWhileMaximum(lexer, {"\""}, 8)
        quotes == Consumed(quoted) - Consumed(lexer)
    IN
    CASE quotes = 1 -> TakeNormalString(quoted)
      [] quotes = 2 -> Push(quoted, "STRING", "None")
      [] quotes \in RawStringOpening -> TakeRawString(quoted)
      [] quotes \in 6..8 -> Push(quoted, "RAW_STRING", "None")
      [] OTHER -> Fail(quoted, "take_string reaches unreachable!()")

TakeHole(lexer) ==
    IF First(lexer) # "?"
    THEN Fail(lexer, "take_hole fails assert_eq!")
    ELSE Push(TakeWhile(Take(lexer), Name), "HOLE", "None")

TakeLower(lexer) == Push(TakeWhile(lexer, Name), "LOWER", "None")

TakeUpper(lexer) == Push(TakeWhile(Take(lexer), Name), "UPPER", "None")

TakeIntegerOrNumber(lexer) ==
    LET error ==
            IF First(lexer) = "0" /\ Second(lexer) = "0"
            THEN "Too many leading zeros"
            ELSE "None"
        integer == TakeWhile(lexer, Digit \cup {"_"})
    IN
    IF First(integer) = "x" THEN
        Push(TakeWhile(Take(integer), HexDigit), "INTEGER", error)
    ELSE IF First(integer) = "." /\ Second(integer) = "." THEN
        Push(integer, "INTEGER", error)
    ELSE
        LET fraction ==
                IF First(integer) = "."
                THEN TakeWhile(Take(integer), Digit \cup {"_"})
                ELSE integer
            exponent ==
                IF First(fraction) \in {"e", "E"} THEN
                    LET marker == Take(fraction)
                        signed == IF First(marker) \in {"+", "-"} THEN Take(marker) ELSE marker
                    IN TakeWhile(signed, Digit \cup {"_"})
                ELSE fraction
            kind ==
                IF First(integer) = "." \/ First(fraction) \in {"e", "E"}
                THEN "NUMBER"
                ELSE "INTEGER"
        IN Push(exponent, kind, error)

TakeTokenImpl(lexer) ==
    LET character == First(lexer) IN
    IF character \in Whitespace THEN Push(lexer, "ERROR", "Expected a token")
    ELSE IF character = "-" /\ Second(lexer) = "-" THEN Push(lexer, "ERROR", "Expected a token")
    ELSE IF character = "{" /\ Second(lexer) = "-" THEN Push(lexer, "ERROR", "Expected a token")
    ELSE IF character = "(" THEN TakeOperatorNameOrLeftParenthesis(lexer)
    ELSE IF character = ")" THEN TakeKind(lexer, "RIGHT_PARENTHESIS")
    ELSE IF character = "{" THEN TakeKind(lexer, "LEFT_CURLY")
    ELSE IF character = "}" THEN TakeKind(lexer, "RIGHT_CURLY")
    ELSE IF character = "," THEN TakeKind(lexer, "COMMA")
    ELSE IF character = "?" /\ Second(lexer) \notin Name THEN TakeOperator(lexer)
    ELSE IF character = "_" /\ Second(lexer) \notin Name THEN
        IF Mutant = "underscore_takes_nothing"
        THEN Push(lexer, "UNDERSCORE", "None")
        ELSE TakeKind(lexer, "UNDERSCORE")
    ELSE IF character = "'" THEN TakeChar(lexer)
    ELSE IF character = "\"" THEN TakeString(lexer)
    ELSE IF character = "?" THEN TakeHole(lexer)
    ELSE IF character \in LowerStart THEN TakeLower(lexer)
    ELSE IF character \in UpperStart THEN TakeUpper(lexer)
    ELSE IF character \in OperatorCharacter THEN TakeOperator(lexer)
    ELSE IF character \in Digit THEN TakeIntegerOrNumber(lexer)
    ELSE Push(Take(lexer), "ERROR", "Invalid token")

TakeToken(lexer) == TakeQualifier(TakeAnnotation(TakeTokenImpl(lexer)))

New(input) ==
    LET start == [line |-> 1, column |-> 1]
        lexer ==
            [source |-> input,
             index |-> 1,
             position |-> start,
             annotation |-> 0,
             qualifier |-> 0,
             qualifierPosition |-> start,
             tokens |-> <<>>,
             fault |-> "None"]
    IN TakeQualifier(TakeAnnotation(lexer))

RECURSIVE LexLoop(_)
LexLoop(lexer) ==
    IF IsEof(lexer) \/ lexer.fault # "None" THEN lexer
    ELSE
        LET next == TakeToken(lexer) IN
        IF next.fault = "None" /\ Consumed(next) = Consumed(lexer)
        THEN Fail(next, "lex loops forever")
        ELSE LexLoop(next)

Finish(lexer) ==
    IF lexer.fault # "None" THEN lexer
    ELSE IF lexer.qualifier > lexer.annotation
    THEN Push(lexer, "END_OF_FILE", "Unexpected end of file")
    ELSE Push(lexer, "END_OF_FILE", "None")

Lex(input) == Finish(LexLoop(New(input)))

Init ==
    /\ source = <<>>
    /\ lexed = Lex(<<>>)

Next ==
    /\ Len(source) < MaxLength
    /\ \E character \in Alphabet :
        LET input == Append(source, character) IN
        /\ source' = input
        /\ lexed' = Lex(input)

Spec == Init /\ [][Next]_vars

Tokens == lexed.tokens

Start(index) == IF index = 1 THEN 0 ELSE Tokens[index - 1].token

At(offset, text) ==
    /\ offset + Len(text) <= Len(source)
    /\ \A index \in 1..Len(text) : source[offset + index] = text[index]

RECURSIVE LineCommentEnd(_)
LineCommentEnd(offset) ==
    IF offset >= Len(source) \/ source[offset + 1] = "\n"
    THEN offset
    ELSE LineCommentEnd(offset + 1)

RECURSIVE BlockCommentEnd(_, _)
BlockCommentEnd(offset, depth) ==
    IF offset >= Len(source) THEN Len(source)
    ELSE IF At(offset, <<"-", "}">>) THEN
        IF depth = 1 THEN offset + 2 ELSE BlockCommentEnd(offset + 2, depth - 1)
    ELSE IF At(offset, <<"{", "-">>) THEN BlockCommentEnd(offset + 2, depth + 1)
    ELSE BlockCommentEnd(offset + 1, depth)

RECURSIVE TriviaEnd(_)
TriviaEnd(offset) ==
    IF offset < Len(source) /\ source[offset + 1] \in Whitespace THEN TriviaEnd(offset + 1)
    ELSE IF At(offset, <<"-", "-">>) THEN TriviaEnd(LineCommentEnd(offset + 2))
    ELSE IF At(offset, <<"{", "-">>) THEN TriviaEnd(BlockCommentEnd(offset + 2, 1))
    ELSE offset

PositionAt(offset) ==
    LET newlines == {index \in 1..offset : source[index] = "\n"}
        lineStart == IF newlines = {} THEN 0 ELSE CHOOSE index \in newlines : \A other \in newlines : other <= index
    IN [line |-> 1 + Cardinality(newlines), column |-> offset - lineStart + 1]

Panics == {
    "take_string reaches unreachable!()",
    "take_raw_string reaches unreachable!()",
    "take_hole fails assert_eq!"
}

NeverPanics == lexed.fault \notin Panics

LexTerminates == lexed.fault # "lex loops forever"

TokensCoverSource ==
    lexed.fault = "None" =>
        /\ Len(Tokens) > 0
        /\ Tokens[Len(Tokens)].kind = "END_OF_FILE"
        /\ Tokens[Len(Tokens)].token = Len(source)
        /\ \A index \in 1..Len(Tokens) :
            /\ Start(index) <= Tokens[index].annotation
            /\ Tokens[index].annotation <= Tokens[index].qualifier
            /\ Tokens[index].qualifier <= Tokens[index].token
            /\ index < Len(Tokens) => Tokens[index].kind # "END_OF_FILE"

AnnotationsAreTrivia ==
    lexed.fault = "None" /\ TokensCoverSource =>
        \A index \in 1..Len(Tokens) : Tokens[index].annotation = TriviaEnd(Start(index))

PositionsMatchOffsets ==
    lexed.fault = "None" =>
        \A index \in 1..Len(Tokens) :
            Tokens[index].position = PositionAt(Tokens[index].annotation)

=============================================================================

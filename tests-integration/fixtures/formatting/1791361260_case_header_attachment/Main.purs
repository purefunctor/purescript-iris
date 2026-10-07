-- @format width=14
-- @format width=15
-- @format width=40
-- @format width=50
-- @format width=80
-- @format width=40 indent=4
module Main where

abc = case _ of
  0 -> 1
  value -> value

longHeader firstArgument secondArgument = case firstArgument of
  0 -> secondArgument
  value -> case secondArgument of
    0 -> value
    other -> other

commented = -- keep the comment before the case
  case _ of
    value -> value

handler = \value -> case value of
  other -> other

applied = consume case input of
    value -> value
  nextArgument

scrutinee = case combine firstArgument secondArgument of
  value -> "this indivisible body literal must not force the case header onto a new line"

multiple = case combine firstArgument secondArgument, choose thirdArgument fourthArgument of
  first, second -> first

nestedScrutinee = case do
  firstAction
  secondAction
  of
    value -> value

headerComment = case input -- scrutinee explanation
  of
    value -> value

branchComment = case input of -- branch explanation
  value -> value

parenthesized = consume (case combine firstArgument secondArgument of
  value -> value)

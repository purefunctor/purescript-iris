-- @format width=80
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

choose = if firstCondition then firstResult else if secondCondition then secondResult else if thirdCondition then thirdResult else fallback

short = if a then x else if b then y else z

blocks = if firstCondition then do
  firstAction
  secondAction
else if secondCondition then do
  thirdAction
else fallback

commented = if firstCondition then firstResult else -- next condition
  if secondCondition firstArgument then secondResult firstArgument else fallbackResult firstArgument

parenthesized = if firstCondition then firstResult else (if secondCondition firstArgument then secondResult firstArgument else fallback)

nested = if firstCondition then if innerCondition firstArgument then innerResult firstArgument else innerFallback firstArgument else if secondCondition then secondResult else fallback

wrapped = if a then x else if predicate firstArgument secondArgument thirdArgument fourthArgument fifthArgument then y else z

inlineComment = if firstCondition then firstResult else {- next condition -} if secondCondition then secondResult else fallback

standaloneComment = if a then x else
  {- next condition -}
  if b then y else z

operand = firstOperand + if firstCondition then firstResult else if secondCondition then secondResult else fallback

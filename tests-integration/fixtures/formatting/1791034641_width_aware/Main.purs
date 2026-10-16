-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main (longApplication, longRecord, branches, lambda, operators, constrained, Box(..)) where

import Data.Collections (Collection, mapCollection, filterCollection, foldCollection, traverseCollection) as Collections

data Box value = Empty | Box value | Pair value value | Many value value value | Nested (Box value)

longApplication :: forall value. Collection value -> Collection value -> Collection value -> Collection value -> Collection value
longApplication firstCollection secondCollection thirdCollection fourthCollection = combineCollections firstCollection secondCollection thirdCollection fourthCollection

longRecord = { firstField: transform inputValue, secondField: [firstElement, secondElement, thirdElement, fourthElement, fifthElement, sixthElement] }
updated = longRecord { firstField = transform anotherInput, secondField = transform otherInput, nested { field = otherValue } }
branches value = if predicate value then transform firstInput secondInput else fallback anotherInput
lambda = \firstArgument secondArgument thirdArgument -> combine firstArgument secondArgument thirdArgument

rows :: { firstField :: Collection Int, secondField :: Collection String, thirdField :: Collection Boolean | tail }
rows = value :: Collection String

blockHead =
  do
      action
    argument
blockMiddle =
  use do
      action
    argument
blockFinal = use do
  action
brokenApplication = invokeOperation firstArgument secondArgument thirdArgument fourthArgument do
  action
operatorAfterBlock =
  do
    action
  + otherAction

constrained :: forall first second. LongConstraint first => OtherConstraint second => first -> second -> Result first second
type Combined = FirstComponent + SecondComponent + ThirdComponent + FourthComponent + FifthComponent
operators = firstOperand + transform firstArgument secondArgument thirdArgument fourthArgument fifthArgument <*> finalOperand
ticks = firstOperand `combineOperation` secondOperand `firstOperation + secondOperation + thirdOperation + fourthOperation` thirdOperand
operatorDo = firstOperand + do
  action
operatorDoLongChain = firstOperand + secondOperand + thirdOperand + fourthOperand + do
  action
operatorAdoLongChain = firstOperand + secondOperand + thirdOperand + fourthOperand + ado
  value <- action
  in transform value
operatorDoMiddle = firstOperand + do
    action
  + finalOperand
operatorBetweenBlocks =
  do
    action
  + do
    otherAction
commentedOperator = do
  firstOperand
    -- between operand and operator
    + secondOperand
commentedOperatorDo = do
  firstOperand
    -- before an inline block
    + do
      action
operatorPattern value = case value of
  firstElement : secondElement : thirdElement : fourthElement : remainingElements -> transform remainingElements

class Base value <= Derived value
builtinBind = do
  value <- action
  pure value
existingUnicode ∷ ∀ value. value → value
existingUnicode value = value
labels :: forall value. { forall :: value } -> value
labels record = record.forall
symbols = "forall :: <- -> <= => ∀ ∷ ← → ⇐ ⇒"
-- forall :: <- -> <= => ∀ ∷ ← → ⇐ ⇒
customOperators = first <= second ⊕ third <=> fourth
type CustomOperator = First <= Second

local value = do
  action value
  where
    helper = transform firstArgument secondArgument thirdArgument fourthArgument fifthArgument
localLet = let bound = firstArgument in transform bound secondArgument

comments = [ firstElement -- trailing before the comma
           , {- inline -} secondElement
           -- standalone before the final item
           , thirdElement ]
multiline = consume """first

  preserve this indentation


last
""" {- comment

    preserve this too

-} argument

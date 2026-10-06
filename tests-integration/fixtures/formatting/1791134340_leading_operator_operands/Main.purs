-- @format width=20
-- @format width=40
-- @format width=20 indent=1
-- @format width=40 indent=4 unicode=true
module Main where

conditional = firstLongOperand + if condition then firstBranchValue else secondBranchValue
localBinding = firstLongOperand <> let bound = longBindingValue in bound + anotherLongOperand
branching = firstLongOperand <=> case choice of
  First -> firstBranchValue
  Second -> secondBranchValue

guarded value
  | firstLongCondition value && secondLongCondition value = 1
  | otherwise = 2

pattern value = case value of
  Cons firstElement secondElement : remainingElements -> result

typed :: forall value. FirstConstraint value => SecondConstraint value => FirstComponent + SecondComponent -> value -> value
typed value = value

commented = firstLongOperand + -- operand explanation
  transform firstArgument secondArgument
negated = firstLongOperand - (- secondLongOperand)

qualified = firstLongOperand Data.Semigroup.<> secondLongOperand Data.Semigroup.<> thirdLongOperand

offside = do
    action
  Data.Semigroup.<> otherAction

commentedDo = left + -- block explanation
  do
    action
commentedMiddle = first + -- middle operand explanation
  second + do
    action

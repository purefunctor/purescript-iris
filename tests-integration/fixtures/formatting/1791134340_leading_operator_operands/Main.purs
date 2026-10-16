-- @format width=80
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

conditional = firstLongOperand + if conditionHolds firstArgument then firstBranchValue else secondBranchValue
localBinding = firstLongOperand <> let bound = longBindingValue in bound + anotherLongOperand
branching = firstLongOperandWithADescriptiveName <=> case chooseBetweenBranches firstArgument of
  First -> firstBranchValue
  Second -> secondBranchValue

guarded value
  | firstLongCondition value && secondLongCondition value && thirdLongCondition value = 1
  | otherwise = 2

pattern value = case value of
  Cons firstElement secondElement : Cons thirdElement fourthElement : remainingElements -> result

typed :: forall value. FirstConstraint value => SecondConstraint value => FirstComponent + SecondComponent -> value -> value
typed value = value

commented = firstLongOperand + -- operand explanation
  transform firstArgument secondArgument
negated = firstOperandWithAVeryLongDescriptiveName - (- secondOperandWithAVeryLongDescriptiveName)

qualified = firstLongOperand Data.Semigroup.<> secondLongOperand Data.Semigroup.<> thirdLongOperand

offside = do
    action
  Data.Semigroup.<> otherAction

commentedDo = left + -- explains why the following block is combined with the left operand
  do
    action
commentedMiddle = first + -- explains why the middle operand is combined with the first operand
  second + do
    action

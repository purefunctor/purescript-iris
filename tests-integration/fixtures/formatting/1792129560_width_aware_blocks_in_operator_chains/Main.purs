-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

operatorAfterBlock =
  do
    action
  + otherAction

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

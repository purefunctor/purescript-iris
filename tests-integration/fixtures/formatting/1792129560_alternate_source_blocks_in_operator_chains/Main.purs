-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

operatorDo = firstOperand
  + do
    action
operatorDoLongChain = firstOperand +
  secondOperand +
  thirdOperand +
  fourthOperand +
  do
    action

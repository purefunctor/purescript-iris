-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

type Combined = FirstComponent + SecondComponent + ThirdComponent + FourthComponent + FifthComponent
operators = firstOperand + transform firstArgument secondArgument thirdArgument fourthArgument fifthArgument <*> finalOperand
ticks = firstOperand `combineOperation` secondOperand `firstOperation + secondOperation + thirdOperation + fourthOperation` thirdOperand
operatorPattern value = case value of
  firstElement : secondElement : thirdElement : fourthElement : remainingElements -> transform remainingElements

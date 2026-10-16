-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

longRecord = { firstField: transform inputValue, secondField: [firstElement, secondElement, thirdElement, fourthElement, fifthElement, sixthElement] }
updated = longRecord { firstField = transform anotherInput, secondField = transform otherInput, nested { field = otherValue } }
branches value = if predicate value then transform firstInput secondInput else fallback anotherInput
lambda = \firstArgument secondArgument thirdArgument -> combine firstArgument secondArgument thirdArgument

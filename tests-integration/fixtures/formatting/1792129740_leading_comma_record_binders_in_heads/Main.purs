-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

patterns first second = case first, second of
  { firstField: firstValue, secondField: secondValue, thirdField: thirdValue, fourthField: fourthValue }, [ item, rest ] -> firstValue
  _, _ -> fallback

named whole@{ firstField: firstValue, secondField: secondValue, thirdField: thirdValue } = whole
lambda = \{ firstField: firstValue, secondField: secondValue, thirdField: thirdValue } -> firstValue

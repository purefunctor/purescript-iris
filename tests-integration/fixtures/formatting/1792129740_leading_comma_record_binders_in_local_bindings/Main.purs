-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

patternLocal = result
  where
  { firstField: result, secondField: secondValue, thirdField: thirdValue, fourthField: fourthValue } = source

doLocal = do
  let { firstField: result, secondField: secondValue, thirdField: thirdValue, fourthField: fourthValue } = source
  pure result

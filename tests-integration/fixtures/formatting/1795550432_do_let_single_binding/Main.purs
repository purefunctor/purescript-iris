-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

singleBinding = do
  let
    value = 1
  pure value

fitsExactly = do
  let
    result = combine firstArgument secondArgument thirdArgument fourthArgument
  pure result

exceedsByOne = do
  let
    result = combine firstArgument secondArgument thirdArgument fourthArguments
  pure result

twoBindings = do
  let
    first = 1
    second = 2
  pure (first + second)

signatureAndEquation = do
  let
    value :: Int
    value = 1
  pure value

multilineBinding = do
  let
    value = case input of
      Just inner -> inner
      Nothing -> 0
  pure value

adoBinding = ado
  value <- action
  let
    doubled = value + value
  in doubled

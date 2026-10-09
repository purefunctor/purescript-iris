-- @format width=30
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

top = let y = 1 in do
  action y

statement = do
  x <- let y = 1 in do
    action y
  pure x

branches value = case value of
  A -> let y = 1 in do
    action y
  B -> if condition then do
    firstAction
    secondAction
  else fallback

conditional = if condition then do
  action
else fallback

binding = let y = do
                action
          in y

empty = ado
  {- before the result -} in 1

emptyLine = ado
  -- before the result
  in 2

letCase = let y = 1 in case y of
  value -> value

conditionalCase =
  if condition
  then case value of
    A -> first
  else do
    fallback

lambdaCase = \value -> case value of
  A -> do
    firstAction
  B -> ado
    result <- secondAction
    in result

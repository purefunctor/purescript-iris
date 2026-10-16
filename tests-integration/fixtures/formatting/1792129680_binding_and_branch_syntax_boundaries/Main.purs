-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

value :: Int
value = 1
local = let value :: Int
            value = 1
        in value
patternLocal = let Tuple a b = pair in a
guarded value | Just result <- lookup value, result > 0 = result
conditional = if condition then 1 else 2
caseResult = case value of
  Just result -> result
multipleCase = case first, second of
  Just result, Nothing -> result
lambda = \value -> value

-- @format width=20 incomplete=false
-- @format width=40 indent=1
-- @format width=40 indent=4 unicode=true
-- @format width=120
module Main where

arrayGuard value
  | [ firstElementName, secondElementName, thirdElementName ] == value = 1
  | otherwise = 2

recordGuard value
  | { firstFieldName: firstArgument, secondFieldName: secondArgument } == value, predicate value = 1
  | otherwise = 2

branch value = case value of
  result | [ firstElementName, secondElementName ] <- result, predicate result -> 1
  _ -> 2

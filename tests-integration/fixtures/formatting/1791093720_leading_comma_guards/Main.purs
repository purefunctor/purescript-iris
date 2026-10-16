-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

arrayGuard value
  | [ firstElementName, secondElementName, thirdElementName, fourthElementName, fifthElementName ] == value = 1
  | otherwise = 2

recordGuard value
  | { firstFieldName: firstArgument, secondFieldName: secondArgument, thirdFieldName: thirdArgument } == value, predicate value = 1
  | otherwise = 2

branch value = case value of
  result | [ firstElementName, secondElementName, thirdElementName, fourthElementName ] <- result, predicate result -> 1
  _ -> 2

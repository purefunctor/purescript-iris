module Main where

import Prelude

compareIntegers :: Int -> Int -> String
compareIntegers left right = case compare left right of
  LT -> "less"
  EQ -> "equal"
  GT -> "greater"

compareStrings :: String -> String -> String
compareStrings left right = case compare left right of
  EQ -> "equal"
  GT -> "greater"
  LT -> "less"

compareNumbers :: Number -> Number -> String
compareNumbers left right = case compare left right of
  GT -> "greater"
  EQ -> "equal"
  _ -> "less"

numberIsGreater :: Number -> Number -> Boolean
numberIsGreater left right = case compare left right of
  GT -> true
  _ -> false

computedIsLess :: Int -> Int -> Boolean
computedIsLess left right = case compare (left + 1) (right * 2) of
  LT -> true
  _ -> false

arrayIsLess :: Array Int -> Array Int -> Boolean
arrayIsLess left right = case compare left right of
  LT -> true
  _ -> false

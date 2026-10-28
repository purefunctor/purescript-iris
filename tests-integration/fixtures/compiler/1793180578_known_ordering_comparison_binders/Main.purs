module Main where

import Prelude

isLess :: Int -> Int -> String
isLess left right = case compare left right of
  LT -> "less"
  other -> "not less"

describeOrdering :: Int -> Int -> String
describeOrdering left right = case compare left right of
  LT -> "less"
  other -> show other

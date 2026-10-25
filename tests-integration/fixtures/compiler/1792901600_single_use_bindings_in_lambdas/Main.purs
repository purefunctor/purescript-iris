module Main where

import Prelude

sharedIntegerAdd :: Int -> Int -> Int -> Int
sharedIntegerAdd left right =
  let total = left + right
  in \value -> total + value

sharedRecord :: Int -> Unit -> { value :: Int }
sharedRecord value =
  let record = { value }
  in \_ -> record

directUse :: Int -> Int -> Int
directUse left right =
  let total = left + right
  in total * 2

branchUse :: Boolean -> Int -> Int -> Int
branchUse flag left right =
  let total = left + right
  in if flag then total else 0

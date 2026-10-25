module Main where

import Prelude

sharedAppend :: String -> Unit -> String
sharedAppend value =
  let doubled = value <> value
  in \_ -> doubled

sharedEqual :: String -> String -> Int -> Int
sharedEqual left right =
  let matches = left == right
  in \value -> if matches then value else 0

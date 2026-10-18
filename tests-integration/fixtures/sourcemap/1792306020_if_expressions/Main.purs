module Main where

import Prelude

classify :: Int -> Int
classify value =
  if value < 0 then
    let magnitude = negate value in magnitude + magnitude
  else
    value

label :: Int -> String
label value = "value " <> (if value < 0 then "negative" else "positive")

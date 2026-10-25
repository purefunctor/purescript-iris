module Main where

import Prelude

sharedFunction :: Int -> Int -> Int
sharedFunction offset =
  let shift = \value -> value + offset
  in \value -> shift value

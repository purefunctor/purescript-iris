module Main where

import Prelude

pairSum :: { left :: Int, right :: Int } -> Int
pairSum pair =
  let
    { left, right } = pair
  in
    left + right

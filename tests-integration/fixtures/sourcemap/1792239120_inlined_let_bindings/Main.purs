module Main where

import Prelude

offset :: Int -> Int
offset value =
  let
    doubled = value * 2
  in
    1 + doubled

module Main where

import Prelude

describe :: Int -> String
describe count =
  let
    doubled = count * 2
  in
    case doubled of
      0 -> "none"
      _ -> label doubled
  where
  label value = "count " <> show value

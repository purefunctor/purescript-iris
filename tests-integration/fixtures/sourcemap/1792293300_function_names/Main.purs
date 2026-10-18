module Main where

import Prelude

scale' :: Int -> Int -> Int
scale' factor value = factor * value

offsets :: Int -> Int
offsets value = shift value + shift (value + 1)
  where
  shift amount = amount + 10

countdown :: Int -> Int
countdown start = go start 0
  where
  go 0 total = total
  go count total = go (count - 1) (total + count)

increments :: Array Int -> Array Int
increments values = map (\value -> value + 1) values

describe :: forall a. Show a => a -> a -> String
describe first second = show first <> " and " <> show second

exclaim :: forall a. Show a => a -> String
exclaim = show >>> \text -> text <> "!"

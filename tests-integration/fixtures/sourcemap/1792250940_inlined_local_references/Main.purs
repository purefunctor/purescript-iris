module Main where

import Prelude

data Box = Box Int

unbox :: Box -> Int
unbox box = helper box
  where
  helper (Box value) = value + 1

size :: Array Int -> Int
size _ = 0

count :: Array Int -> Int
count values =
  let
    measure = size
  in
    measure values

newtype Meters = Meters Int

newtype Distance = Distance Meters

distance :: Int -> Distance
distance = Meters >>> Distance

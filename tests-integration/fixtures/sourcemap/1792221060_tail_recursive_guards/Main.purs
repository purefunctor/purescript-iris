module Main where

import Prelude

sumTo :: Int -> Int -> Int
sumTo total count
  | count <= 0 = total
  | otherwise = sumTo (total + count) (count - 1)

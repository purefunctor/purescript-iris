module Main where

data Pair a b = Pair a b

withinBinder :: Pair Int Int -> Int
withinBinder = \(Pair x x) -> x

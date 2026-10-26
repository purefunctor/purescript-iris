module Main where

data Pair a b = Pair a b

multipleScrutinees :: Int -> Int -> Int
multipleScrutinees a b = case a, b of
  x, x -> x

singleScrutinee :: Pair Int Int -> Int
singleScrutinee p = case p of
  Pair x x -> x

separateBranches :: Int -> Int -> Int
separateBranches a b = case a, b of
  0, x -> x
  x, _ -> x

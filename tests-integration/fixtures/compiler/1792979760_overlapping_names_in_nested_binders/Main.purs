module Main where

data Pair a b = Pair a b

infixr 6 Pair as /\

acrossArguments :: Int -> Pair Int Int -> Int
acrossArguments x (Pair _ x) = x

constructor :: Pair Int Int -> Int
constructor (Pair x x) = x

operator :: Pair Int Int -> Int
operator (x /\ x) = x

array :: Array Int -> Int
array [x, x] = x
array _ = 0

named :: Pair Int Int -> Int
named x@(Pair x _) = 0

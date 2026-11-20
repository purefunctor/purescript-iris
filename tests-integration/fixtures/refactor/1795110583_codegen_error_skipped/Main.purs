module Main where

add :: Int -> Int -> Int
add a b = a

infixl 6 add as +

broken :: Int -> Int
broken n =
  let
    two = 2
    m = n +
  in
    two

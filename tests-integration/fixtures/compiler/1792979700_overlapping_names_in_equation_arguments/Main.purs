module Main where

topLevel :: Int -> Int -> Int
topLevel x x = x

whereBound :: Int -> Int
whereBound a = go a a
  where
  go y y = y

class Combine a where
  combine :: a -> a -> a

instance Combine Int where
  combine z z = z

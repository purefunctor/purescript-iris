module Main where

import Prelude

integerDivide :: Int -> Int -> Int
integerDivide left right = left / right

integerModulo :: Int -> Int -> Int
integerModulo left right = left `mod` right

halveAndRemainder :: Int -> Int
halveAndRemainder value = value / 2 + value `mod` 2

partiallyAppliedDivide :: Int -> Int
partiallyAppliedDivide = div 100

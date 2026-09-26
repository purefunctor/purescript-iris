module Main where

import Prelude

test :: Int
test = -2147483648

test2 :: Int
test2 = -(0x80000000)

test3 :: Int
test3 = negate 2_147_483_648

test4 :: Int -> Boolean
test4 (-2147483648) = true
test4 _ = false

test5 :: Int
test5 = -(-2147483648)

test6 :: Int
test6 = -2147483647

test7 :: Int
test7 = 2147483647

test8 :: Int
test8 = let negate value = value in -42

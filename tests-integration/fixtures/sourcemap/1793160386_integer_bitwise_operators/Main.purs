module Main where

import Data.Int.Bits (complement, shl, shr, zshr, (.&.), (.^.), (.|.))

masked :: Int -> Int -> Int
masked flags mask = (flags .&. mask) .|. (flags .^. complement mask)

shifted :: Int -> Int -> Int
shifted value count = shl value count .|. shr value count .^. zshr value count

module Main where

import Data.Int.Bits (complement, shl, shr, zshr, (.&.), (.^.), (.|.))

bitAnd :: Int -> Int -> Int
bitAnd left right = left .&. right

bitOr :: Int -> Int -> Int
bitOr left right = left .|. right

bitXor :: Int -> Int -> Int
bitXor left right = left .^. right

shiftLeft :: Int -> Int -> Int
shiftLeft value count = shl value count

shiftRight :: Int -> Int -> Int
shiftRight value count = shr value count

zeroFillShiftRight :: Int -> Int -> Int
zeroFillShiftRight value count = zshr value count

bitComplement :: Int -> Int
bitComplement value = complement value

lowByte :: Int -> Int
lowByte value = (value .&. 255) .|. (complement 0 .&. 0)

partiallyAppliedAnd :: Int -> Int
partiallyAppliedAnd = (.&.) 15

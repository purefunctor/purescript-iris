module Main where

import Prelude

import Data.Array (length, unsafeIndex)
import Partial.Unsafe (unsafePartial)

arrayLength :: Array String -> Int
arrayLength values = length values

firstValue :: Array String -> String
firstValue values = unsafePartial (unsafeIndex values 0)

lastValue :: Array String -> String
lastValue values = unsafePartial (unsafeIndex values (length values - 1))

indexWith :: Partial => Array String -> Int -> String
indexWith values = unsafeIndex values

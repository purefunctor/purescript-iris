module Main where

import Prelude

import Data.Array as Array
import Partial.Unsafe (unsafePartial)

lastValue :: Array Int -> Int
lastValue values = unsafePartial (Array.unsafeIndex values (Array.length values - 1))

surrounded :: Array Int -> Array Int -> Array Int
surrounded left right = [ 0 ] <> left <> right <> [ Array.length right ]

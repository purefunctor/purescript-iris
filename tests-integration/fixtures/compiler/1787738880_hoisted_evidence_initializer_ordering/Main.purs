module Main where

import Data.Eq as Eq

compareArrays :: Array Int -> Array Int -> Boolean
compareArrays left right =
  if Eq.eq left right then Eq.eq right left else false

initialized :: Boolean
initialized = compareArrays [1] [1]

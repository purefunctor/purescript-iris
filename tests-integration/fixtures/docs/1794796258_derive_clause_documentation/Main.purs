module Main where

import Wrapping (class Wrap)

-- | A local type with an attached derive clause.
data Box = Box
  derive (Wrap)

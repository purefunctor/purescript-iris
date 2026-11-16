module Nullary where

class Nul where
  nul :: Int

-- A nullary class constraint has no unknown, but purs generalizes it.
test x = nul

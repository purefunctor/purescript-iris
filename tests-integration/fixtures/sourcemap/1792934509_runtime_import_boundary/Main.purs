module Main (Recursive, seed) where

import Data.Eq (class Eq, eq)

foreign import seed :: Int

data Recursive

instance eqRecursive :: Eq Recursive where
  eq left right = eq left right

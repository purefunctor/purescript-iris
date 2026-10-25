module Lookalike where

import Prelude

data Wrapped = Wrapped Int

instance eqInt :: Eq Wrapped where
  eq _ _ = true

instance ordInt :: Ord Wrapped where
  compare _ _ = GT

lessThan :: Int -> Int -> Boolean
lessThan _ _ = false

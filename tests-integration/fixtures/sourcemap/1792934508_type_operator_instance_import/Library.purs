module Library (Pair(..)) where

import Data.Show (class Show)

data Pair left right = Pair left right

instance showPair :: Show (Pair left right) where
  show _ = "pair"

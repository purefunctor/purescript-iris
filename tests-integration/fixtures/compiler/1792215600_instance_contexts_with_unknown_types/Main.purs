module Main where

import Data.Show (class Show)

class Pick :: Type -> Constraint
class Pick a where
  pick :: a -> String

instance Show (Int String) => Pick Int where
  pick _ = "int"
else instance Pick a where
  pick _ = "other"

picksInt :: String
picksInt = pick 1

picksString :: String
picksString = pick "string"

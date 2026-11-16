module Main where

import Prelude

data Colour = Red | Green | Blue
  derive (Eq, Ord)

newtype Name = Name String
  derive (Eq)
  derive (Ord)

data Void
  derive (Eq)

sameColour :: Boolean
sameColour = Red == Red && Red < Green

sameName :: Boolean
sameName = Name "Alice" == Name "Alice" && Name "Alice" < Name "Bob"

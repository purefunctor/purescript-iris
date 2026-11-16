module Main where

import Prelude

data Colour = Red | Green | Blue
  derive (Ord)
  derive (Eq)

data Control = First | Second
derive instance Eq Control
derive instance Ord Control

equal = Red == Red
unequal = Red == Blue
ordered = Red < Green && Green < Blue && not (Blue < Red)
control = First /= Second && First < Second

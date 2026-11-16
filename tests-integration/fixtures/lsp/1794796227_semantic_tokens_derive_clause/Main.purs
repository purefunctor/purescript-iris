module Main where

import Lib as Lib

class Equal a

data Colour = Red
  derive (Equal, Lib.Show)

-- semantic tokens

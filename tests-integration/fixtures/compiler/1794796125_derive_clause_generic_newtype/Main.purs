module Main where

import Data.Generic.Rep (class Generic)
import Data.Newtype (class Newtype)

data Direction = North | South
  derive (Generic)

data Pair a b = Pair a b
  derive (Generic)

newtype Wrapper a = Wrapper a
  derive (Generic, Newtype)

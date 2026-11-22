module Main
  ( value
  , value
  , Choice(..)
  , Choice(First)
  , Pair(Pair, Pair)
  , Split(Left)
  , Split(Right)
  , module Library
  , module Library
  ) where

import Library

value = 0

data Choice = First | Second

data Pair = Pair

data Split = Left | Right

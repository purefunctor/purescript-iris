module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray
import Data.Maybe (Maybe)

type Accessed =
  { length :: Int
  , first :: Maybe Int
  , written :: Boolean
  , last :: Maybe Int
  , head :: Maybe Int
  }

accessed :: Array Int -> Accessed
accessed values = ST.run do
  array <- STArray.thaw values
  length <- STArray.length array
  first <- STArray.peek 0 array
  written <- STArray.poke (length - 1) 0 array
  last <- STArray.pop array
  head <- STArray.shift array
  pure { length, first, written, last, head }

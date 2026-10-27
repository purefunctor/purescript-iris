module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray
import Data.Maybe (Maybe)

type Reads =
  { length :: Int
  , first :: Maybe Int
  , last :: Maybe Int
  , past :: Maybe Int
  , negative :: Maybe Int
  }

reads :: Array Int -> Reads
reads values = ST.run do
  array <- STArray.thaw values
  length <- STArray.length array
  first <- STArray.peek 0 array
  last <- STArray.peek (length - 1) array
  past <- STArray.peek length array
  negative <- STArray.peek (-1) array
  pure { length, first, last, past, negative }

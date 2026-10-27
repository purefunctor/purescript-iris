module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray
import Data.Maybe (Maybe)

type Removals =
  { last :: Maybe Int
  , first :: Maybe Int
  , emptyLast :: Maybe Int
  , emptyFirst :: Maybe Int
  , final :: Array Int
  }

removals :: Array Int -> Removals
removals values = ST.run do
  array <- STArray.thaw values
  last <- STArray.pop array
  first <- STArray.shift array
  empty <- STArray.new
  emptyLast <- STArray.pop empty
  emptyFirst <- STArray.shift empty
  final <- STArray.freeze array
  pure { last, first, emptyLast, emptyFirst, final }

module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray
import Data.Maybe (Maybe)

foreign import traced :: forall a. String -> a -> a

type Removals =
  { length :: Int
  , last :: Maybe Int
  , first :: Maybe Int
  , final :: Array Int
  }

removals :: Array Int -> Removals
removals values = ST.run do
  array <- STArray.thaw values
  length <- STArray.length (traced "length array" array)
  last <- STArray.pop (traced "pop array" array)
  first <- STArray.shift (traced "shift array" array)
  final <- STArray.freeze array
  pure { length, last, first, final }

module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray
import Data.Maybe (Maybe)

foreign import traced :: forall a. String -> a -> a

type Accesses =
  { peeked :: Maybe Int
  , poked :: Boolean
  , final :: Array Int
  }

accesses :: Array Int -> Accesses
accesses values = ST.run do
  array <- STArray.thaw values
  peeked <- STArray.peek (traced "peek index" 1) (traced "peek array" array)
  poked <- STArray.poke (traced "poke index" 5) (traced "poke value" 9) (traced "poke array" array)
  final <- STArray.freeze array
  pure { peeked, poked, final }

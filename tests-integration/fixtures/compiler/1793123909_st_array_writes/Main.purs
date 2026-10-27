module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray

type Writes =
  { inside :: Boolean
  , past :: Boolean
  , negative :: Boolean
  , modified :: Boolean
  , unmodified :: Boolean
  , final :: Array Int
  }

writes :: Array Int -> Writes
writes values = ST.run do
  array <- STArray.thaw values
  inside <- STArray.poke 1 10 array
  past <- STArray.poke 3 20 array
  negative <- STArray.poke (-1) 30 array
  _ <- STArray.poke 0 40 array
  modified <- STArray.modify 2 (_ * 2) array
  unmodified <- STArray.modify 3 (_ * 2) array
  final <- STArray.freeze array
  pure { inside, past, negative, modified, unmodified, final }

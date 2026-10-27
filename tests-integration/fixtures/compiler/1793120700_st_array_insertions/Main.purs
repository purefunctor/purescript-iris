module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray

inserted :: Array Int -> Array Int
inserted more = STArray.run do
  array <- STArray.new
  _ <- STArray.push 2 array
  _ <- STArray.pushAll [ 3, 4 ] array
  _ <- STArray.pushAll (more <> [ 9 ]) array
  _ <- STArray.unshift 1 array
  _ <- STArray.unshiftAll more array
  pure array

lengths :: Array Int
lengths = ST.run do
  array <- STArray.new
  pushed <- STArray.push 1 array
  pushedAll <- STArray.pushAll [ 2, 3 ] array
  unshifted <- STArray.unshift 0 array
  unshiftedAll <- STArray.unshiftAll [ -2, -1 ] array
  pure [ pushed, pushedAll, unshifted, unshiftedAll ]

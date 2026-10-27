module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray

literal :: Array Int
literal = ST.run do
  array <- STArray.unsafeThaw [ 1, 2 ]
  _ <- STArray.push 3 array
  STArray.unsafeFreeze array

shared :: Array Int -> Array Int
shared values = ST.run do
  array <- STArray.unsafeThaw values
  frozen <- STArray.unsafeFreeze array
  _ <- STArray.push 0 array
  pure frozen

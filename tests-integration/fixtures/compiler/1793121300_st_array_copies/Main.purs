module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray

type Copies =
  { original :: Array Int
  , frozen :: Array Int
  , cloned :: Array Int
  , final :: Array Int
  }

copies :: Array Int -> Copies
copies original = ST.run do
  array <- STArray.thaw original
  _ <- STArray.push 3 array
  frozen <- STArray.freeze array
  clone <- STArray.clone array
  _ <- STArray.push 4 array
  _ <- STArray.push 5 clone
  cloned <- STArray.freeze clone
  final <- STArray.freeze array
  pure { original, frozen, cloned, final }

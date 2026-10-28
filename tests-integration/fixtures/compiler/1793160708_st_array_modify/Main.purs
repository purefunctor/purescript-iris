module Main where

import Prelude

import Control.Monad.ST (ST)
import Data.Array.ST (STArray)
import Data.Array.ST as STArray

foreign import traced :: forall a. String -> a -> a

modifyAt :: forall h. Int -> (Int -> Int) -> STArray h Int -> ST h Boolean
modifyAt index function array =
  STArray.modify (traced "index" index) (traced "function" function) (traced "array" array)

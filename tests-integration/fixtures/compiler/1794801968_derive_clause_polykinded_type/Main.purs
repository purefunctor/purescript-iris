module Main where

import Prelude

import Data.Foldable (class Foldable)
import Data.Traversable (class Traversable)

data Proxy a
  derive (Functor, Foldable, Traversable)

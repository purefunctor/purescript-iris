module Main where

import Prelude

import Data.Bifunctor (class Bifunctor)
import Data.Foldable (class Foldable)
import Library (Box, Pair)

derive instance Functor Box

derive instance Foldable Box

derive instance Bifunctor Pair

module Main where

import Prelude
import Data.Bifunctor (class Bifunctor, bimap)
import Data.Bifoldable (class Bifoldable, bifoldMap)
import Data.Bitraversable (class Bitraversable, bitraverse)
import Data.Maybe (Maybe(..))

data Pair a b = Pair a b
  derive (Bifunctor, Bifoldable, Bitraversable)

derive instance (Eq a, Eq b) => Eq (Pair a b)

mapped = bimap (_ + 7) (_ <> "!") (Pair 3 "right") == Pair 10 "right!"
folded = bifoldMap show identity (Pair 3 "right")
traversed = bitraverse (\a -> Just (a + 7)) (\b -> Just (b <> "!")) (Pair 3 "right") == Just (Pair 10 "right!")

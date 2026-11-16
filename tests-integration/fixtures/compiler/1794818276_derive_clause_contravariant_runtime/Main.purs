module Main where

import Prelude
import Data.Functor.Contravariant (class Contravariant, cmap)
import Data.Profunctor (class Profunctor, dimap)

newtype Predicate a = Predicate (a -> Boolean)
  derive (Contravariant)

newtype Transform a b = Transform (a -> b)
  derive (Profunctor)

predicate = case cmap (_ + 4) (Predicate (_ > 7)) of
  Predicate run -> run 5 && not (run 2)

transformed = case dimap (_ + 3) (_ <> "!") (Transform show) of
  Transform run -> run 8

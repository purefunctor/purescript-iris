module Main where

import Prelude
import Data.Eq (class Eq1, eq1)
import Data.Ord (class Ord1, compare1)
import Data.Foldable (class Foldable, foldl)
import Data.Traversable (class Traversable, traverse)
import Data.Functor.Contravariant (class Contravariant, cmap)
import Data.Maybe (Maybe(..))

data Phantom a = Phantom Int
  derive (Eq1, Ord1, Functor, Foldable, Traversable, Contravariant)

derive instance Eq (Phantom a)
derive instance Ord (Phantom a)

mapped :: Int
mapped = case map (_ + 2) (Phantom 13 :: Phantom Int) of Phantom n -> n

folded :: Int
folded = foldl (-) 17 (Phantom 13 :: Phantom Int)

traversed :: Int
traversed = case traverse (\n -> Just (n + 1)) (Phantom 13 :: Phantom Int) of
  Just (Phantom n) -> n
  Nothing -> 0

contramapped :: Int
contramapped = case cmap (_ + 2) (Phantom 13 :: Phantom Int) of Phantom n -> n

equal :: Boolean
equal = eq1 (Phantom 13 :: Phantom Int) (Phantom 14)

ordered :: Boolean
ordered = compare1 (Phantom 13 :: Phantom Int) (Phantom 14) == LT

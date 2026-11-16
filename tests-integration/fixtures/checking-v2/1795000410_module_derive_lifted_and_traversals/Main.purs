module Main where

import Data.Eq (class Eq, class Eq1)
import Data.Foldable (class Foldable)
import Data.Traversable (class Traversable)

data Pair a = Pair a a

derive instance eqPair :: Eq a => Eq (Pair a)

derive instance eq1Pair :: Eq1 Pair

data Opaque a = Opaque (a -> Int)

derive instance eq1Opaque :: Eq1 Opaque

data Tree a = Leaf | Node (Array (Tree a)) a { label :: a }

derive instance foldableTree :: Foldable Tree

derive instance traversableTree :: Traversable Tree

data Reader a = Reader (Int -> a)

derive instance foldableReader :: Foldable Reader

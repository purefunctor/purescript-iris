module Main where

import Prelude
import Data.Eq (class Eq1, eq1)
import Data.Ord (class Ord1, compare1)
import Data.Foldable (class Foldable, foldMap)
import Data.Traversable (class Traversable, traverse)
import Data.Maybe (Maybe(..))

data List a = Nil | Cons a (List a)
  derive (Functor, Foldable, Traversable, Eq1, Ord1)

derive instance Eq a => Eq (List a)
derive instance Ord a => Ord (List a)

mapped = map (_ + 4) (Cons 3 (Cons 8 Nil)) == Cons 7 (Cons 12 Nil)
folded = foldMap show (Cons 3 (Cons 8 Nil))
traversed = traverse (\value -> if value < 5 then Just (value + 1) else Nothing) (Cons 1 (Cons 3 Nil)) == Just (Cons 2 (Cons 4 Nil))
rejected = traverse (\value -> if value < 5 then Just value else Nothing) (Cons 1 (Cons 8 Nil)) == Nothing
equal = eq1 (Cons 1 Nil) (Cons 1 Nil)
unequal = eq1 (Cons 1 Nil) (Cons 2 Nil)
ordered = compare1 (Cons 1 Nil) (Cons 2 Nil) == LT

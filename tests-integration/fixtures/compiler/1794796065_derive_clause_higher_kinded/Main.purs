module Main where

import Prelude

import Data.Bifunctor (class Bifunctor)
import Data.Foldable (class Foldable)
import Data.Traversable (class Traversable)

data List a = Nil | Cons a (List a)
  derive (Functor, Foldable, Traversable)

derive instance Eq a => Eq (List a)

data Either2 a b = Left2 a | Right2 b
  derive (Bifunctor)

module Main where

import Prelude

import Data.Bifunctor (class Bifunctor)
import Data.Eq (class Eq1)
import Data.Foldable (class Foldable)

foreign import data Opaque :: Type -> Type

foreign import data OpaquePair :: Type -> Type -> Type

instance Eq a => Eq (Opaque a) where
  eq _ _ = true

derive instance Functor Opaque

derive instance Foldable Opaque

derive instance Bifunctor OpaquePair

derive instance Eq1 Opaque

module Data.Eq where

import Data.HeytingAlgebra as HA

class Eq a where
  eq :: a -> a -> Boolean

class Eq1 :: (Type -> Type) -> Constraint
class Eq1 f where
  eq1 :: forall a. Eq a => f a -> f a -> Boolean

foreign import eqIntImpl :: Int -> Int -> Boolean

instance eqInt :: Eq Int where
  eq = eqIntImpl

module Data.Functor where

class Functor :: (Type -> Type) -> Constraint
class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

foreign import arrayMap :: forall a b. (a -> b) -> Array a -> Array b

instance functorArray :: Functor Array where
  map = arrayMap

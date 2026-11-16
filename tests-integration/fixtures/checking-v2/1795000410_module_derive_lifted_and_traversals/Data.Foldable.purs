module Data.Foldable where

class Foldable :: (Type -> Type) -> Constraint
class Foldable f where
  foldr :: forall a b. (a -> b -> b) -> b -> f a -> b

foreign import foldrArray :: forall a b. (a -> b -> b) -> b -> Array a -> b

instance foldableArray :: Foldable Array where
  foldr = foldrArray

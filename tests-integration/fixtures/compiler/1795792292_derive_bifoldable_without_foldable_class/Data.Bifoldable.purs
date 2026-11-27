module Data.Bifoldable where

import Data.Monoid (class Monoid)

class Bifoldable :: (Type -> Type -> Type) -> Constraint
class Bifoldable p where
  bifoldr :: forall a b c. (a -> c -> c) -> (b -> c -> c) -> c -> p a b -> c
  bifoldl :: forall a b c. (c -> a -> c) -> (c -> b -> c) -> c -> p a b -> c
  bifoldMap :: forall m a b. Monoid m => (a -> m) -> (b -> m) -> p a b -> m

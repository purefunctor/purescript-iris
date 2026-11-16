module Main where

class Eq a where
  eq :: a -> a -> Boolean

class Functor :: (Type -> Type) -> Constraint
class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

newtype IdentityT m a = IdentityT (m a)

derive newtype instance eqIdentityT :: Eq (m a) => Eq (IdentityT m a)

derive newtype instance functorIdentityT :: Functor m => Functor (IdentityT m)

newtype Function a b = Function (a -> b)

derive newtype instance functorFunction :: Functor (Function a)

newtype Swapped a b = Swapped (b -> a)

derive newtype instance functorSwapped :: Functor (Swapped a)

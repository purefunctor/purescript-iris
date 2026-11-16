module RecConstraintTyped where

class Shw a where
  shw :: a -> String

k :: forall a b. a -> b -> a
k x _ = x

-- Control: the same binding with a signature.
f :: forall a. Shw a => a -> String
f x = k (shw x) (f x)

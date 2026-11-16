module RecConstraint where

class Shw a where
  shw :: a -> String

k :: forall a b. a -> b -> a
k x _ = x

-- A recursive binding that needs a constraint.
f x = k (shw x) (f x)

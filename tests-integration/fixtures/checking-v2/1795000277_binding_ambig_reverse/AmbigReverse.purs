module AmbigReverse where

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  finv :: b -> a

k :: forall a b. a -> b -> a
k x _ = x

-- F ?a ?x and Shw ?a: ?x is in the type, but F's fundep runs a -> b,
-- so ?a is not determined by the type.
test x = k x (shw (finv x))

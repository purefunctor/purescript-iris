module AmbigFundep where

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

k :: forall a b. a -> b -> a
k x _ = x

-- Shw ?b is determined from x through F's fundep: not ambiguous.
test x = k x (shw (fget x))

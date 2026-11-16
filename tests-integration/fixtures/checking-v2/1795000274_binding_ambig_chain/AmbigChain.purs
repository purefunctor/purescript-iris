module AmbigChain where

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

k :: forall a b. a -> b -> a
k x _ = x

-- F a b, F b c, Shw c: determined through a chain of fundeps.
test x = k x (shw (fget (fget x)))

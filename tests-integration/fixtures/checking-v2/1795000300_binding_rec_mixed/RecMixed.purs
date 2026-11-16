module RecMixed where

class Shw a where
  shw :: a -> String

k :: forall a b. a -> b -> a
k x _ = x

-- f has a signature; g is untyped and needs Shw. They form one group.
f :: forall a. Shw a => a -> String
f x = g x
g x = k (shw x) f

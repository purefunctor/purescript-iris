module RecMixedLeakUse where

k :: forall a b. a -> b -> a
k x _ = x

f :: forall a. a -> a
f x = g x
g x = k x f

-- Intended: g :: forall b. b -> b, so this is well typed.
useG :: Int
useG = g 1

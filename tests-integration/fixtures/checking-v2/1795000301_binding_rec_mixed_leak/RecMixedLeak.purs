module RecMixedLeak where

k :: forall a b. a -> b -> a
k x _ = x

-- f has a signature; g is untyped. Checking f solves g's type with f's skolem.
f :: forall a. a -> a
f x = g x
g x = k x f

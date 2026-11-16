module RecMixedLeak where
foreign import k :: forall a b. a -> b -> a
-- f is signed; g is not. purs keeps them in one binding group (no split at
-- signatures), so g is checked with f's skolem a0 and g : a0 -> a0 escapes.
f :: forall a. a -> a
f x = g x
g x = k x f

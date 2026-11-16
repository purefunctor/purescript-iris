module RecMixedCtl where
foreign import k :: forall a b. a -> b -> a
-- Control: g is signed too, so no unsigned unknown takes f's skolem.
f :: forall a. a -> a
f x = g x
g :: forall a. a -> a
g x = k x f

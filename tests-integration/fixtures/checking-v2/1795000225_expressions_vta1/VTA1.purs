module VTA1 where
class C a where
  c :: a -> Int
instance C Int where
  c _ = 1
-- A constraint in front of a visible binder: purs instantiates only invisible
-- foralls before looking for the visible one.
f :: forall a. C a => forall @b. a -> b -> Int
f x _ = c x
t :: Int
t = f @String 1 "s"

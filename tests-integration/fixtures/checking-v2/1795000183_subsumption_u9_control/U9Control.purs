module U9Control where
identity :: forall a. a -> a
identity x = x
foreign import g :: (forall a. a -> a) -> Int
-- The same call with f's type known: f's argument is a polytype, nothing escapes.
t = (\(f :: (forall a. a -> a) -> Int) -> f identity) g

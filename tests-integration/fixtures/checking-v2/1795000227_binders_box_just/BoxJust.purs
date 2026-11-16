module BoxJust where
data Maybe a = Just a | Nothing
foreign import add :: Int -> Int -> Int
-- A constructor binder nested under a polymorphic constructor field.
data Box = Box (forall a. Maybe a)
f :: Box -> Int
f (Box (Just x)) = add x 1
f _ = 0

module GuardCtorPoly where
data Maybe a = Just a | Nothing
foreign import poly :: forall a. Maybe a
foreign import add :: Int -> Int -> Int
-- A constructor binder in a pattern guard on a polymorphic expression.
t :: Int -> Int
t n | Just x <- poly = add n x
t n = n

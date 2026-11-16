module GuardMono where
foreign import idp :: forall a. a -> a
-- A pattern guard with a monotype annotation on a polymorphic expression.
t :: Int -> Int
t n | (g :: Int -> Int) <- idp = g n

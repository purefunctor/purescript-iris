module GuardCtorMono where
data Maybe a = Just a | Nothing
foreign import mono :: Maybe Int
foreign import add :: Int -> Int -> Int
-- Control: a pattern guard on a monomorphic expression.
t :: Int -> Int
t n | Just x <- mono = add n x
t n = n

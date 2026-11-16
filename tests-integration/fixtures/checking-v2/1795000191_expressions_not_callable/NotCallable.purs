module NotCallable where
foreign import one :: Int
foreign import plus :: Int -> Int -> Int
-- DV-4.1: the head is not a function and the argument is also ill-typed.
test = one (plus "a" 2)

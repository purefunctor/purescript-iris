module ImmediateCycleLazy where

k :: forall a b. a -> b -> a
k a _ = a

-- y refers to x outside a function body, but x is a lambda, so
-- initializing x first and then y is safe at runtime.
x :: Int -> Int
x = \n -> y n n

y :: Int -> Int -> Int
y = k x

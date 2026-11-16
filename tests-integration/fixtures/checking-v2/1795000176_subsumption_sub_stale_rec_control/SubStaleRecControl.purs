module SubStaleRecControl where
foreign import usePoly :: { c :: forall a. a -> a } -> Int
foreign import useMono :: { c :: Int -> Int } -> Int
foreign import add :: Int -> Int -> Int
-- The same uses with r's type written on the binder.
t = \(r :: { c :: forall a. a -> a }) -> add (usePoly r) (useMono r)

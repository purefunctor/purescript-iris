module SubStaleRecSwap where
foreign import usePoly :: { c :: forall a. a -> a } -> Int
foreign import useMono :: { c :: Int -> Int } -> Int
foreign import add :: Int -> Int -> Int
-- Same as SubStaleRec, uses swapped: useMono r first.
t = \r -> add (useMono r) (usePoly r)

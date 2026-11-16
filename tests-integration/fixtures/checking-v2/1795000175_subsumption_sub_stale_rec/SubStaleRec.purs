module SubStaleRec where
foreign import usePoly :: { c :: forall a. a -> a } -> Int
foreign import useMono :: { c :: Int -> Int } -> Int
foreign import add :: Int -> Int -> Int
-- r : ?r. The first use solves ?r := { c :: forall a. a -> a }; the second
-- needs { c :: forall a. a -> a } to subsume { c :: Int -> Int }, which it does.
t = \r -> add (usePoly r) (useMono r)

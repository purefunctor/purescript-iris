module SubStaleFn where
foreign import usePoly :: (Int -> forall a. a -> a) -> Int
foreign import useMono :: (Int -> Int -> Int) -> Int
foreign import add :: Int -> Int -> Int
-- f : ?f. The first use solves ?f := Int -> forall a. a -> a; the second needs
-- that type to subsume Int -> Int -> Int, which it does (instantiate the result).
t = \f -> add (usePoly f) (useMono f)

module SubStaleFnControl where
foreign import usePoly :: (Int -> forall a. a -> a) -> Int
foreign import useMono :: (Int -> Int -> Int) -> Int
foreign import add :: Int -> Int -> Int
-- The same uses with f's type written on the binder.
t = \(f :: Int -> forall a. a -> a) -> add (usePoly f) (useMono f)

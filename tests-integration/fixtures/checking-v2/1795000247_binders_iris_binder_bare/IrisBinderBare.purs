module IrisBinderBare where
foreign import len :: String -> Int
foreign import neg :: Int -> Int
-- DV-4.3 without the record wrapper.
h :: (Int -> Int) -> Int
h = \(f :: forall a. a -> a) -> len (f "a")
result :: Int
result = h neg

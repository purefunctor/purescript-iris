module EscExistUse where
-- An existential: unpack hands its continuation a value of a hidden type.
foreign import unpack :: forall z. (forall a. a -> (a -> String) -> z) -> z
foreign import len :: String -> Int
leak = (\f -> f (\x _ -> x)) unpack
-- If leak's hidden type were generalized, this would pass 42 to len.
claim :: Int
claim = len leak

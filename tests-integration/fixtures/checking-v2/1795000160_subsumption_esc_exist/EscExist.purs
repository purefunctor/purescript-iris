module EscExist where
-- An existential: unpack hands its continuation a value of a hidden type.
foreign import unpack :: forall z. (forall a. a -> (a -> String) -> z) -> z
foreign import len :: String -> Int
-- If the hidden type escapes, its value (a String at runtime) can be passed anywhere.
leak = (\f -> f (\x _ -> x)) unpack

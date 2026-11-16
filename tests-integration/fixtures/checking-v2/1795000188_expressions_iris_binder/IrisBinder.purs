module IrisBinder where
foreign import len :: String -> Int
-- Localise IrisEscape: a typed lambda binder checked against a known argument type.
h :: { x :: Int -> Int } -> Int
h = \(r :: { x :: forall a. a -> a }) -> len (r.x "a")

module IrisSubtype where
foreign import len :: String -> Int
-- Localise IrisEscape: plain subsumption, no case and no binder annotation.
f :: { x :: forall a. a -> a } -> Int
f r = len (r.x "a")
g :: { x :: Int -> Int } -> Int
g = f

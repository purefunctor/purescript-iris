module GuardVarPoly where
foreign import idp :: forall a. a -> a
foreign import both :: Int -> String -> Int
-- The same through a pattern guard.
t :: Int -> Int
t n | f <- idp = both (f n) (f "a")

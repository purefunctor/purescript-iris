module VarPoly where
data Maybe a = Just a | Nothing
foreign import add :: Int -> Int -> Int
-- PB-5.3 without an as-pattern: the first case solves the lambda's ?u to a polytype
-- through a typed binder; the second case on the same variable reads ?u as stored.
f = \v -> case v of
  (_ :: forall a. Maybe a) -> case v of
    Just x -> add x 1
    _ -> 0

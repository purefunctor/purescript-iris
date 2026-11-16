module NamedPoly where
data Maybe a = Just a | Nothing
foreign import add :: Int -> Int -> Int
-- A named binder takes the column type as stored: here the lambda's unknown ?u,
-- which the typed binder solves to forall a. Maybe a. The case on `all` then sees
-- ?u, which instantiatePolyTypeWithUnknowns does not read through the substitution.
f = \all@(m :: forall a. Maybe a) -> case all of
  Just x -> add x 1
  _ -> 0

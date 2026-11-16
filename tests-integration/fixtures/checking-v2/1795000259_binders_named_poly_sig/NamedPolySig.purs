module NamedPolySig where
data Maybe a = Just a | Nothing
foreign import add :: Int -> Int -> Int
-- Control for NamedPoly: with a signature, the column type is the polytype itself.
f :: (forall a. Maybe a) -> Int
f = \all@(m :: forall a. Maybe a) -> case all of
  Just x -> add x 1
  _ -> 0

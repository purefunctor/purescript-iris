module NamedPolyM where
data Maybe a = Just a | Nothing
foreign import add :: Int -> Int -> Int
-- Control for NamedPoly: the case is on the inner name m.
f = \all@(m :: forall a. Maybe a) -> case m of
  Just x -> add x 1
  _ -> 0

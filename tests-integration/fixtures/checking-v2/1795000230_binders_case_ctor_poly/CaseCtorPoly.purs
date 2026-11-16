module CaseCtorPoly where
data Maybe a = Just a | Nothing
foreign import poly :: forall a. Maybe a
foreign import add :: Int -> Int -> Int
-- Control for GuardCtorPoly: the same match as a case.
t :: Int -> Int
t n = case poly of
  Just x -> add n x
  _ -> n

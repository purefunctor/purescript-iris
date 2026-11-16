module LamMono where
-- BoxMono without a constructor: a lambda argument of type forall a. a -> a
-- with a binder annotated Int -> Int.
h :: (forall a. a -> a) -> Int
h = \(g :: Int -> Int) -> g 1

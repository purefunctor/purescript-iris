module U1Swap where
-- Branches swapped: the monomorphic branch comes first.
t = \f -> f (if true then { x: \y -> y } else { x: ((\y -> y) :: forall a. a -> a) })

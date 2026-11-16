module U1Mono where
-- Control for U1Swap: a monomorphic signature for the same program.
t :: forall b r. ({ x :: b -> b } -> r) -> r
t = \f -> f (if true then { x: \y -> y } else { x: ((\y -> y) :: forall a. a -> a) })

module U1Annot where
-- Control for U1/U1Swap: the same if, checked against the polymorphic record type.
t :: ({ x :: forall a. a -> a } -> Int) -> Int
t = \f -> f (if true then { x: ((\y -> y) :: forall a. a -> a) } else { x: \y -> y })

module U1Bare where
-- U1 without any application: the split is in infer'[IfThenElse] itself.
t = if true then { x: ((\y -> y) :: forall a. a -> a) } else { x: \y -> y }

module U1Arrow where
-- Same argument, but the head is a known function with an unknown argument type.
app :: forall a b. (a -> b) -> a -> b
app g v = g v
t = \f -> app f (if true then { x: ((\y -> y) :: forall a. a -> a) } else { x: \y -> y })

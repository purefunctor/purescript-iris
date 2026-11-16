module U1 where
-- Unknown head; the argument is an if whose first branch has a polymorphic field.
t = \f -> f (if true then { x: ((\y -> y) :: forall a. a -> a) } else { x: \y -> y })

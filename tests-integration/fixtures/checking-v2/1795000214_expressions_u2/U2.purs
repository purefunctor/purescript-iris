module U2 where
data B = T | F
-- Unknown head; the argument is a case whose first branch has a polymorphic field.
t = \f b -> f (case b of
  T -> { x: ((\y -> y) :: forall a. a -> a) }
  F -> { x: \y -> y })

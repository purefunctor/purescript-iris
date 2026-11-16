module U3 where
-- The argument mentions the head: f (\x -> f x). Both should report an infinite type.
t = \f -> f (\x -> f x)

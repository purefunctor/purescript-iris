module S1 where
data B = T | F
-- The first branch solves the case's result unknown to a type with a polytype
-- field. purs checks the second lambda against that unknown without applying
-- the substitution, so it infers the lambda instead of checking it.
t b = case b of
  T -> \(r :: { x :: forall a. a -> a }) -> r.x 1
  F -> \r -> r.x 2

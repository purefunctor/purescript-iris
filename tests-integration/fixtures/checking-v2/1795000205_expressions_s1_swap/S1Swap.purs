module S1Swap where
data B = T | F
-- S1 with the branches swapped: the unannotated lambda solves the unknown first.
t b = case b of
  F -> \r -> r.x 2
  T -> \(r :: { x :: forall a. a -> a }) -> r.x 1

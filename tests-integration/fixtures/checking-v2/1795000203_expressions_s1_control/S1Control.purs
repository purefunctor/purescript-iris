module S1Control where
data B = T | F
-- S1 with a signature: the same second branch is checked against a known type.
t :: B -> { x :: forall a. a -> a } -> Int
t b = case b of
  T -> \(r :: { x :: forall a. a -> a }) -> r.x 1
  F -> \r -> r.x 2

module InfiniteBranching where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data L a = L a
data R a = R a
data T a b = T a b

newtype N a = N (T (N (L a)) (N (R a)))

-- Every unwrap doubles the queue with new types: the derivation is infinite,
-- and below any depth limit it is exponentially wide.
test :: forall a b. N a -> N b
test = coerce

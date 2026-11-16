module RoleMutualPhantom where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

-- A recursive group in which `a` never reaches a value.
data P a = P (Q a)
data Q a = Q (P a) | Q0

p2 :: P Int -> P String
p2 = coerce

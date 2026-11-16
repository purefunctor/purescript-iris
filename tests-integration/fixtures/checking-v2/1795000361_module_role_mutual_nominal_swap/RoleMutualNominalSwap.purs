module RoleMutualNominalSwap where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

data T a = T a
type role T nominal

-- RoleMutualNominal with the two group members in the other order.
data S a = S (R a) | S0
data R a = R (S a) | R0 (T a)

s2 :: S Int -> S N
s2 = coerce

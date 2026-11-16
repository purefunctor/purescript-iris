module RoleMutualNominal where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data T a = T a
type role T nominal

-- R and S form a group; R reaches T's nominal parameter, and S reaches it through R.
data R a = R (S a) | R0 (T a)
data S a = S (R a) | S0

newtype N = N Int

s2 :: S Int -> S N
s2 = coerce

module HKUnknownHint where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data L a = L a
data M a = M a

data P (f :: Type -> Type) = P

type role P representational

-- Coercible L M is saturated with a fresh unknown: Coercible (L ?t) (M ?t).
test :: P L -> P M
test = coerce

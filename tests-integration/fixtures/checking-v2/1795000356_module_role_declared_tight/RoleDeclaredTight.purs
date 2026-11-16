module RoleDeclaredTight where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

-- Inferred representational, declared nominal (stricter): accepted, stored nominal.
data T a = T a
type role T nominal

t2 :: T Int -> T N
t2 = coerce

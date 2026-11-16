module RoleExternA where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

foreign import data A :: Type -> Type
type role A representational

data U a = U (A a)

direct :: A Int -> A N
direct = coerce

viaU :: U Int -> U N
viaU = coerce

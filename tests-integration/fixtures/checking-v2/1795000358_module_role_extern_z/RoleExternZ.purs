module RoleExternZ where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

foreign import data Z :: Type -> Type
type role Z representational

data U a = U (Z a)

direct :: Z Int -> Z N
direct = coerce

viaU :: U Int -> U N
viaU = coerce

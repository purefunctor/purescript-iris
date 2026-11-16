module RoleNominalPropControl where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data T a = T a
type role T nominal

newtype N = N Int

direct :: T Int -> T N
direct = coerce

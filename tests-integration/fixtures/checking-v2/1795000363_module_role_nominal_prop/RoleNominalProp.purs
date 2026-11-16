module RoleNominalProp where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

-- T holds an `a`, but its author declared the parameter nominal.
data T a = T a
type role T nominal

-- D inherits T's nominal role.
data D a = D (T a)

newtype N = N Int

-- Representational would allow this; nominal must not.
d2 :: D Int -> D N
d2 = coerce

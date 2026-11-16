module RoleDeclaredLoose where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

-- The declared phantom role is looser than what the group needs.
data T a = T (U a)
type role T phantom
data U a = U (T a) | U2 a

module RoleVarHead where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

-- The argument of a variable head is nominal.
data D f a = D (f a)

d2 :: D Array Int -> D Array N
d2 = coerce

module RoleConstrained where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

class Show a where
  show :: a -> String

-- A variable in a constraint argument is nominal.
data D a = D (Show a => a)

d2 :: D Int -> D N
d2 = coerce

module RoleOverapplyControl where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Maybe a = Nothing | Just a

foreign import data Id :: forall k. k -> k

-- Same field, but the argument sits inside Id's one declared parameter.
data D a = D (Id (Maybe a))

d2 :: D Int -> D String
d2 = coerce

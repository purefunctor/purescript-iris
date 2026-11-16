module RoleOverapplyControl where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Maybe a = Nothing | Just a

foreign import data Id :: forall k. k -> k

-- Control: the same coercion with Id applied to one argument is rejected.
direct :: Id Int -> Id String
direct = coerce

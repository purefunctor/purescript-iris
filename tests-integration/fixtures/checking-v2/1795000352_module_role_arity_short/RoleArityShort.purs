module RoleArityShort where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

-- Two parameters, one declared role.
data T a b = T a b
type role T nominal

t2 :: T Int Int -> T Int String
t2 = coerce

claim :: String
claim = case t2 (T 1 2) of T _ s -> s

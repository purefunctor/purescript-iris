module RolePhantomProxy where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

-- Proxy's parameter is phantom, so D's is too.
data D a = D (Proxy a)

d2 :: D Int -> D String
d2 = coerce

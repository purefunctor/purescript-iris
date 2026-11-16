module GivenKickOut where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Proxy (f :: Type -> Type) = Proxy

test :: forall f g a b. Coercible a (f b) => Coercible f g => Proxy f -> a -> g b
test _ = coerce

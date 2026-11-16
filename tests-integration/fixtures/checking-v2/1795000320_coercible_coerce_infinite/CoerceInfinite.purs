module CoerceInfinite where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N a = N (a -> N a)

test :: forall a b. N a -> N b
test = coerce

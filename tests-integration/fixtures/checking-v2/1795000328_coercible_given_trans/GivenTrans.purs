module GivenTrans where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

test :: forall a b c. Coercible a b => Coercible b c => a -> c
test = coerce

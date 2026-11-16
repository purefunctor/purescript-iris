module CoerceInfer where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

-- No signature: Coercible ?a ?b reaches the generalize pass.
test x = coerce x

use :: N
use = test 1

module GivenInsoluble where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data D a = D a

type role D nominal

test :: forall a b. Coercible (D a) (D b) => D a -> D b
test = coerce

use :: D Boolean
use = test (D true)

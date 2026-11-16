module CoerceBeforeFundep where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

class F a b | a -> b where
  fget :: a -> b

instance F Int String where
  fget _ = "s"

newtype N = N String

-- coerce's placeholder (Coercible ?b N) is visited before fget's (F Int ?b),
-- which is what solves ?b := String.
test :: N
test = coerce (fget 1)

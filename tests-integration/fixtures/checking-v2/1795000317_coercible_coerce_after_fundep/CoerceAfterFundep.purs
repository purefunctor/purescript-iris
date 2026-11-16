module CoerceAfterFundep where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

class F a b | a -> b where
  fget :: a -> b

instance F Int String where
  fget _ = "s"

newtype N = N String

applyFlipped :: forall a b. a -> (a -> b) -> b
applyFlipped x f = f x

-- Control: the same constraints, but fget's placeholder is visited first.
test :: N
test = applyFlipped (fget 1) coerce

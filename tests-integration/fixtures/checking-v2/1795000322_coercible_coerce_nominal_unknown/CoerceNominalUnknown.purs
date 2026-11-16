module CoerceNominalUnknown where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data D a = D a

type role D nominal

class F a b | a -> b where
  fget :: a -> D b

instance F Int Int where
  fget _ = D 1

-- Coercible (D ?b) (D Int): decompose compares the nominal argument as stored.
test :: D Int
test = coerce (fget 1)

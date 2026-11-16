module CoerceRowTail where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

class F a (r :: Row Type) | a -> r where
  fget :: a -> Record (x :: Int | r)

instance F Int () where
  fget _ = { x: 1 }

-- Coercible (x :: Int | ?r) (x :: Int): canonRow throws on the bare tail ?r.
test :: { x :: Int }
test = coerce (fget 1)

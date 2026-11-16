module CoerceResidualHead where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype Id a = Id a

class G a (f :: Type -> Type) | a -> f where
  gget :: a -> f Int

instance G Int Id where
  gget _ = Id 1

-- Coercible (?f Int) Int is irreducible (canonApplicationLeft); G Int ?f,
-- visited later, solves ?f := Id.
test :: Int
test = coerce (gget 1)

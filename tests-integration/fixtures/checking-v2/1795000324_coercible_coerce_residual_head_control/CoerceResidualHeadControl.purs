module CoerceResidualHeadControl where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype Id a = Id a

class G a (f :: Type -> Type) | a -> f where
  gget :: a -> f Int

instance G Int Id where
  gget _ = Id 1

applyFlipped :: forall a b. a -> (a -> b) -> b
applyFlipped x f = f x

test :: Int
test = applyFlipped (gget 1) coerce

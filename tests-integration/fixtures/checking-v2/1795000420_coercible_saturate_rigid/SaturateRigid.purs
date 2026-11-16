module SaturateRigid where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data D :: Type -> Type
data D a = D a

type role D nominal

newtype K :: Type -> Type
newtype K a = K (D Int)

data P :: (Type -> Type) -> Type
data P f = P

type role P representational

-- Coercible K D needs K t ~ D t for every t, that is D Int ~ D t, which is
-- false. The argument that saturates K and D must stay rigid.
test :: P K -> P D
test = coerce

newtype K2 :: Type -> Type
newtype K2 a = K2 (D a)

-- Control: K2 t ~ D t holds for every t, so the coercion is accepted.
accepted :: P K2 -> P D
accepted = coerce

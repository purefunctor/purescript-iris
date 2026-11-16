module NominalUnify where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data D :: Type -> Type
data D a = D Int

type role D nominal

mk :: forall b. D b
mk = D 1

-- Coercible (D ?b) (D Int) with D nominal holds only for ?b := Int.
test :: D Int
test = coerce mk

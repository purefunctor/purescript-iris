module CoerceNominalFree where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data D a = D a

type role D nominal

foreign import mk :: forall b. Int -> D b

-- Nothing but this Coercible wanted fixes ?b; Coercible (D ?b) (D Int) holds
-- with ?b := Int.
test :: D Int
test = coerce (mk 1)

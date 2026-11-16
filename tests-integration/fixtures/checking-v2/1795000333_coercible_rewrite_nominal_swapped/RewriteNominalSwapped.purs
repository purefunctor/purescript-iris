module RewriteNominalSwapped where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Unit = Unit

newtype Age = Age Int

data D :: Type -> Type -> Type
data D a b = D b

type role D nominal representational

test :: forall a c. Coercible a Int => Coercible c (D Int Unit) => c -> D a Unit
test = coerce

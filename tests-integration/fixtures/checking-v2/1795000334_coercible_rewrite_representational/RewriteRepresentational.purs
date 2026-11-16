module RewriteRepresentational where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Unit = Unit

data D :: Type -> Type -> Type
data D a b = D b

type role D representational nominal

-- Control: a is now D's representational argument, so the rewrite is allowed.
test :: forall a c. Coercible c (D Int Unit) => Coercible a Int => c -> D a Unit
test = coerce

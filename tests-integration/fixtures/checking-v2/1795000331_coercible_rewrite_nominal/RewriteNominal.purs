module RewriteNominal where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Unit = Unit

newtype Age = Age Int

data D :: Type -> Type -> Type
data D a b = D b

type role D nominal representational

-- Coercible a Int may rewrite only representational occurrences of a, and
-- a is D's nominal argument here.
test :: forall a c. Coercible c (D Int Unit) => Coercible a Int => c -> D a Unit
test = coerce

-- test at a = Age, c = D Int Unit: the coercion that direct (below) shows
-- purs rejects.
claim :: D Int Unit -> D Age Unit
claim = test

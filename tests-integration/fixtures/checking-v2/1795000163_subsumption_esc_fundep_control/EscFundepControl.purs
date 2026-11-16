module EscFundepControl where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
class Same :: Type -> Type -> Constraint
class Same a b | a -> b
instance Same a a
foreign import coerce :: forall a b. a -> b
conv :: forall r x. Same x x => ST r x -> ST r x
conv = coerce
-- The same constraint, but the result type is fixed by unification before the escape check.
leak = run (conv (newRef 1))

module EscFundep where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
class Same :: Type -> Type -> Constraint
class Same a b | a -> b
instance Same a a
foreign import coerce :: forall a b. a -> b
conv :: forall r x y. Same x y => ST r x -> ST r y
conv = coerce
-- The result type ?y is fixed only when entailment improves Same (Ref r Int) ?y,
-- after the escape check has read the elaborated types.
leak = run (conv (newRef 1))

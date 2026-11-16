module EscConstraint where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
class Shw :: Type -> Type -> Constraint
class Shw a b
foreign import coerce :: forall a b. a -> b
sh :: forall r a b. Shw a b => ST r a -> ST r b
sh = coerce
-- Shw (Ref r Int) ?b mentions run's skolem r and stays unsolved, so it is generalized.
leak = run (sh (newRef 1))

module EscConstraintControl where
foreign import data ST :: Type -> Type -> Type
foreign import mkST :: forall r a. a -> ST r a
foreign import run :: forall a. (forall r. ST r a) -> a
class Shw :: Type -> Type -> Constraint
class Shw a b
foreign import coerce :: forall a b. a -> b
sh :: forall r a b. Shw a b => ST r a -> ST r b
sh = coerce
-- The generalized constraint Shw Int ?b mentions no skolem.
ok = run (sh (mkST 1))

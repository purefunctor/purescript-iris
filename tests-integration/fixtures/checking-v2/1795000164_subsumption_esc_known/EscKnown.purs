module EscKnown where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
-- The same escape through a known head: the skolem comes from subsuming run's type.
app :: forall a b. (a -> b) -> a -> b
app f x = f x
leak = app run (newRef 1)

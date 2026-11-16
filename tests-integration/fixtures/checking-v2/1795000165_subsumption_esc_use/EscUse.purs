module EscUse where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
-- What type does each compiler give the escaped value, and can a caller use it?
leak = (\f -> f (newRef 1)) run
foreign import readRef :: forall r a. Ref r a -> ST r a
useIt :: Int
useIt = run (readRef leak)

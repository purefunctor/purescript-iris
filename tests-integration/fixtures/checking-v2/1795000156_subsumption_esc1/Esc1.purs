module Esc1 where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
-- Direct escape: run's argument would return a Ref tied to its own region.
leak = run (newRef 1)

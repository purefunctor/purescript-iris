module Esc1 where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
-- Skolem escape with no unknown head (area 3 territory).
leak = run (newRef 1)

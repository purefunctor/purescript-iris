module Esc2 where
foreign import data ST :: Type -> Type -> Type
foreign import data Ref :: Type -> Type -> Type
foreign import newRef :: forall r a. a -> ST r (Ref r a)
foreign import run :: forall a. (forall r. ST r a) -> a
-- U6's shape with a value whose escape matters (area 4, DV-4.7).
leak = (\f -> f (newRef 1)) run

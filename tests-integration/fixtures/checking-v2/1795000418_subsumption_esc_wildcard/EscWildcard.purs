module EscWildcard where
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
-- The wildcard is solved only when Same improves the result type, so the
-- region r escapes into the signature.
leak :: _
leak = run (conv (newRef 1))
-- Control: a wildcard solved to the signature's own variable does not escape.
own :: forall a. { x :: a } -> { x :: a, y :: _ }
own r = { x: r.x, y: r.x }

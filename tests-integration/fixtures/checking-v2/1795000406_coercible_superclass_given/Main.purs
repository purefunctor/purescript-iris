module Main where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

class Newtype :: Type -> Type -> Constraint
class Coercible t a <= Newtype t a | t -> a

wrap :: forall t a. Newtype t a => a -> t
wrap = coerce

unwrap :: forall t a. Newtype t a => t -> a
unwrap = coerce

class Unrelated :: Type -> Type -> Constraint
class Unrelated t a

unrelated :: forall t a. Unrelated t a => a -> t
unrelated = coerce

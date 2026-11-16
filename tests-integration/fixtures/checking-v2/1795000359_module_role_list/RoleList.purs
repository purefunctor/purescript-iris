module RoleList where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

data L a = Nil | Cons a (L a)

l2 :: L Int -> L N
l2 = coerce

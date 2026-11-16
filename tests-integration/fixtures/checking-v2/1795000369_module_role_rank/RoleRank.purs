module RoleRank where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype N = N Int

-- z is rebound by the forall, so T's parameter is phantom.
data T z = T (forall z. z -> z)

t2 :: T Int -> T String
t2 = coerce

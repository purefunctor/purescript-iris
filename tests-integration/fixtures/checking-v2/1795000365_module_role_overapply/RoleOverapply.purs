module RoleOverapply where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Maybe a = Nothing | Just a

-- A foreign type whose result kind is a variable, so it can take more
-- arguments than its kind has arrows: Id Maybe a.
foreign import data Id :: forall k. k -> k
foreign import mk :: forall a. Maybe a -> Id Maybe a
foreign import un :: forall a. Id Maybe a -> Maybe a

data D a = D (Id Maybe a)

d :: D Int
d = D (mk (Just 1))

d2 :: D String
d2 = coerce d

claim :: Maybe String
claim = case d2 of D x -> un x

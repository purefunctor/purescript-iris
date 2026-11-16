module RoleOverapplyDirect where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

data Maybe a = Nothing | Just a

foreign import data Id :: forall k. k -> k

-- No data type in between: the Coercible solver decomposes Id Maybe a itself.
direct :: Id Maybe Int -> Id Maybe String
direct = coerce

foreign import mk :: forall a. Maybe a -> Id Maybe a
foreign import un :: forall a. Id Maybe a -> Maybe a

claim :: Maybe String
claim = un (direct (mk (Just 1)))

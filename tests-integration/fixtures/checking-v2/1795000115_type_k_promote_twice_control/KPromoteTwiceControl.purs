module KPromoteTwiceControl where

import KProxy (Proxy)

foreign import data Pair :: Type -> Type -> Type

-- Same as KPromoteTwice, but T is applied directly, with no synonym parameter to promote into.
foreign import data T :: forall a. Proxy (Pair a a) -> Type
foreign import data Q :: Proxy (Pair Int String)

type Y = T Q

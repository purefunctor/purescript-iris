module KindedNotTypeControl where

import KProxy (Proxy)

data Maybe a = Nothing | Just a

-- The same ill-kinded kind as a binder annotation: checkIsSaturatedType rejects it.
foreign import f :: forall (a :: Maybe). Proxy a -> Int

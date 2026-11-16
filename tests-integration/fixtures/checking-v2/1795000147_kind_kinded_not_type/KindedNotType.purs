module KindedNotType where

import KProxy (Proxy)

data Maybe a = Nothing | Just a

-- Maybe has kind Type -> Type, so it is not a kind; (_ :: Maybe) is ill-kinded.
foreign import f :: Proxy (_ :: Maybe) -> Int

module KPromoteDataControl where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

-- The same, with Q's kind fixed, so D's kind has one unknown that occurs once.
data D :: Proxy (P (Q :: Type -> Type)) -> Type
data D p = D

type Y r = D r

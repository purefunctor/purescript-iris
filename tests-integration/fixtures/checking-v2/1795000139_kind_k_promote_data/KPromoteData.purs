module KPromoteData where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

-- D's kind is forall k1 (k2 :: k1). Proxy (P @k1 @k2 (Q @k1)) -> Type: k1 occurs twice.
data D :: Proxy (P Q) -> Type
data D p = D

-- Correct: Y :: forall k1 (k2 :: k1). Proxy (P @k1 @k2 (Q @k1)) -> Type.
type Y r = D r

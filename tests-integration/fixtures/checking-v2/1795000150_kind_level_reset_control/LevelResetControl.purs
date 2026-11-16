module LevelResetControl where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

-- The same kind without the forall x binder.
data D :: Proxy (P Q) -> Type
data D p = D

module LevelResetUseControl where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

-- The same use, with no forall in D's signature (the second argument fixed to Proxy Int).
data D :: Proxy (P Q) -> Proxy Int -> Type
data D p q = D

foreign import data S :: Proxy Int

type Y r = D r S

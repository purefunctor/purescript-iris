module LevelResetValueControl where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

foreign import f :: Proxy (P Q) -> Int

foreign import r :: Proxy (P Q)

use :: Int
use = f r

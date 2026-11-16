module LevelResetValue where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

-- The same kinds in a foreign value's type: kindOfWithUnknowns + varIfUnknown generalize in level order.
foreign import f :: forall x. Proxy (P Q) -> Proxy x -> Int

foreign import r :: Proxy (P Q)
foreign import s :: Proxy Int

-- A well-typed use of f.
use :: Int
use = f r s

module LevelResetUse where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

data D :: forall x. Proxy (P Q) -> Proxy x -> Type
data D p q = D

foreign import data S :: Proxy Int

-- Well-kinded: r gets the kind of D's first argument, S fits its second.
type Y r = D r S

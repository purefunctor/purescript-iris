module KOpenOpenControl where

import KProxy (Proxy)

-- Control for KOpenOpen: one open row against a closed one, so no shared rest is created.
-- Correct: X :: Proxy (b :: Type) -> Type.
foreign import data T :: forall r. Proxy (a :: Type | r) -> Proxy r -> Type
foreign import data P :: Proxy (a :: Type, b :: Type)

type X q = T P q

module KRestKindControl where

import KProxy (Proxy)

-- Same closing without the open/open step: accepted.
foreign import data T :: forall r. Proxy (a :: Type | r) -> Proxy r -> Type
foreign import data R :: Proxy (b :: Type)

type X p = T p R

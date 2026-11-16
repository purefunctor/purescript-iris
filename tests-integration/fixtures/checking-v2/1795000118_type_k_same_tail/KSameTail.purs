module KSameTail where

import KProxy (Proxy)

-- Two identical open rows: must be accepted.
foreign import data T :: forall r. Proxy (a :: Type | r) -> Proxy (a :: Type | r) -> Type

type X p = T p p

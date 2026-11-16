module KSameTailControl where

import KProxy (Proxy)

-- The same rows without a common label: accepted.
foreign import data T :: forall r. Proxy r -> Proxy r -> Type

type X p = T p p

module KCycleControl where

import KProxy (Proxy)

-- No common label: r = (b :: Int | r) must be InfiniteKind.
foreign import data T :: forall r. Proxy r -> Proxy (b :: Int | r) -> Type

type X p = T p p

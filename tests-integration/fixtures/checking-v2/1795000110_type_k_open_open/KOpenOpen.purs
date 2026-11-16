module KOpenOpen where

import KProxy (Proxy)

-- Two open rows with different labels: r = (b :: Type | t) and s = (a :: Type | t), with ONE shared t.
-- X's inferred kind shows whether r and s kept the same rest.
foreign import data T :: forall r s. Proxy (a :: Type | r) -> Proxy (b :: Type | s) -> Proxy r -> Proxy s -> Type

type X p q1 q2 = T p p q1 q2

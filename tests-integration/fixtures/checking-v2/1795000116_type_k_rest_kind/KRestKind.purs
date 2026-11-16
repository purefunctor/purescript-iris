module KRestKind where

import KProxy (Proxy)

-- Two open rows with different labels share one fresh rest t: r = (b :: Type | t), s = (a :: Type | t).
-- R closes t = (). Correct: accepted.
foreign import data T :: forall r s. Proxy (a :: Type | r) -> Proxy (b :: Type | s) -> Proxy r -> Type
foreign import data R :: Proxy (b :: Type)

type X p = T p p R

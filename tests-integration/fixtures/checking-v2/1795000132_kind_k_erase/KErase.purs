module KErase where

import KProxy (Proxy)

-- Kind-level twin of Erase.purs.
-- Correct: r = (b :: Int); both rows are then (a :: Record (b :: Int), b :: Int): accepted.
foreign import data T :: forall r. Proxy (a :: Record r | r) -> Proxy (a :: Record (b :: Int) | r) -> Type

type X p = T p p

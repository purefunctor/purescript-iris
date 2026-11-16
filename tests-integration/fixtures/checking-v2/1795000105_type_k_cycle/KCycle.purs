module KCycle where

import KProxy (Proxy)

-- Kind-level twin of Cycle.purs.
-- Correct: r1 = (c :: Int | r2) and r2 = (b :: Int | r1) has no finite solution: InfiniteKind.
foreign import data T :: forall r1 r2. Proxy (a :: Record r1 | r2) -> Proxy (a :: Record (c :: Int | r2), b :: Int | r1) -> Type

type X p = T p p

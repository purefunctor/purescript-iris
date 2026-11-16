module LevelReset where

import KProxy (Proxy)

-- P's k is solved by Q's (newer) j through the argument kind, so k := j' (promoted).
-- a :: k stays unsolved, so the generalized kind needs  forall k2 (k1 :: k2). ...
foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

-- With a forall in the signature: inferKind[ForAll] resets every body unknown to level [u].
data D :: forall x. Proxy (P Q) -> Proxy x -> Type
data D p q = D

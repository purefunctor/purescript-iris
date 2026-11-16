module KLabel where

import KProxy (Proxy)

-- Correct: k = Type from label a, then k = Symbol from label b: KindsDoNotUnify.
foreign import data T :: forall k. Proxy (a :: k, b :: k) -> Type
foreign import data P :: Proxy (a :: Type, b :: Symbol)

type X = T P

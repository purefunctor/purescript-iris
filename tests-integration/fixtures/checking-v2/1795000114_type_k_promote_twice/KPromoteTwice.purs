module KPromoteTwice where

import KProxy (Proxy)

foreign import data Pair :: Type -> Type -> Type

-- p's kind must be Proxy (Pair a a) for ONE a. Correct: X :: forall k. Proxy (Pair k k) -> Type.
foreign import data T :: forall a. Proxy (Pair a a) -> Type

type X p = T p

-- Correct: KindsDoNotUnify (Int vs String), since X needs Pair k k.
foreign import data Q :: Proxy (Pair Int String)

type Y = X Q

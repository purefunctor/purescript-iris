module AmbiguousSynonym where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type
foreign import data R :: Proxy (P Q)

-- R's kind arguments are never fixed: the body is R @?k @?a, with nothing to generalize them into.
type Y = Proxy R

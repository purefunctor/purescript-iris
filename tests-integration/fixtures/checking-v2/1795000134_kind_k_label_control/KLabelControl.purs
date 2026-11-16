module KLabelControl where

import KProxy (Proxy)

-- Same constraint as KLabel, but k is a whole argument, not a row label: must be KindsDoNotUnify.
foreign import data T :: forall k. Proxy k -> Proxy k -> Type
foreign import data A :: Proxy Type
foreign import data B :: Proxy Symbol

type X = T A B

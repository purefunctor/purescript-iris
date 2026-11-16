module KOverwrite where

import KProxy (Proxy)

-- Kind-level twin of Overwrite.purs.
-- Correct: r = (b :: Int) from label a, but the tail says r = (b :: String): KindsDoNotUnify.
foreign import data T :: forall r. Proxy (a :: Record r | r) -> Type
foreign import data P :: Proxy (a :: Record (b :: Int), b :: String)

type X = T P

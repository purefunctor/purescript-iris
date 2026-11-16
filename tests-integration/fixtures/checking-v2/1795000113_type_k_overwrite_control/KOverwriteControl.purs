module KOverwriteControl where

import KProxy (Proxy)

-- Same contradiction with the tail already solved: must be KindsDoNotUnify.
foreign import data T :: Proxy (a :: Record (b :: Int), b :: Int) -> Type
foreign import data P :: Proxy (a :: Record (b :: Int), b :: String)

type X = T P

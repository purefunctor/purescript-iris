module OverlapUse where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

class C a where
  c :: a -> String

-- Rejected at declaration time (PB-9.3): the kind arguments (@k vs @Type) are not apart.
instance cAny :: C (Proxy a) where
  c _ = "cAny"

instance cInt :: C (Proxy Int) where
  c _ = "cInt"

x :: String
x = c (Proxy :: Proxy Int)

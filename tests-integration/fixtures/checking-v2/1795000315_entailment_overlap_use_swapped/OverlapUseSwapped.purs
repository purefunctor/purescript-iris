module OverlapUseSwapped where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

class C a where
  c :: a -> String

-- Accepted at declaration time: the kind arguments (@k vs @Type) count as apart.
instance cInt :: C (Proxy Int) where
  c _ = "cInt"

instance cAny :: C (Proxy a) where
  c _ = "cAny"

x :: String
x = c (Proxy :: Proxy Int)

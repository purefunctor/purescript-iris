module OverlapKindApp where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

class C a where
  c :: a -> Int

instance cAny :: C (Proxy a) where
  c _ = 1

instance cInt :: C (Proxy Int) where
  c _ = 2

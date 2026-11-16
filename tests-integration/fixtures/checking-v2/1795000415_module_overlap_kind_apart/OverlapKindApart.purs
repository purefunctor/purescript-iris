module OverlapKindApart where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

class C :: Type -> Constraint
class C a where
  c :: a -> Int

instance cSymbol :: C (Proxy (a :: Symbol)) where
  c _ = 1

instance cInt :: C (Proxy Int) where
  c _ = 2

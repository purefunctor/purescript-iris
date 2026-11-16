module OverlapControl where

data P (a :: Type) = P

class C a where
  c :: a -> Int

instance cAny :: C (P a) where
  c _ = 1

instance cInt :: C (P Int) where
  c _ = 2

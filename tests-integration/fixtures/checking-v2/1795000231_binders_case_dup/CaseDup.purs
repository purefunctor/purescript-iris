module CaseDup where
data P = P Int Int
foreign import mk :: P
-- Control for GuardDup: the same binder in a case alternative.
t :: Int
t = case mk of
  P x x -> x

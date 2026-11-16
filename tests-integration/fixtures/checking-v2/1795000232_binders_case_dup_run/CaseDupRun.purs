module CaseDupRun where
data P = P Int String
mk :: P
mk = P 1 "s"
-- Control for GuardDupRun: the same binder as a case alternative.
result :: Int
result = case mk of
  P x x -> x

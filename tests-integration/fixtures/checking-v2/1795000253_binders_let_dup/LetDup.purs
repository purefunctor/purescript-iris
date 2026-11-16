module LetDup where
data P = P Int String
mk :: P
mk = P 1 "s"
-- Control for GuardDupRun: the same binder in a let pattern (desugared to a case).
result :: Int
result = let P x x = mk in x

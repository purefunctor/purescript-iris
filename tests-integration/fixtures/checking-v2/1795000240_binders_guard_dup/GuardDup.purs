module GuardDup where
data P = P Int Int
foreign import mk :: P
-- The same name twice in a pattern-guard binder.
t :: Int -> Int
t n | P x x <- mk = x
t n = n

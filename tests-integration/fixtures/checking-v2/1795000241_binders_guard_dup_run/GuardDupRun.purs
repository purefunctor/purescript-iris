module GuardDupRun where
data P = P Int String
mk :: P
mk = P 1 "s"
-- The same name twice in a pattern-guard binder, at two different types.
-- If accepted, x :: Int may be bound to the String.
t :: Int -> Int
t n | P x x <- mk = x
t n = n
result :: Int
result = t 0

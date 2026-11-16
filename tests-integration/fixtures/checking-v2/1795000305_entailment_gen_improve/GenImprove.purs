module GenImprove where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

-- Generalize pass visits F ?x ?r1, then Shw ?r2, then F ?x ?r2.
-- The third is solved by the given F ?x ?r1 and improves ?r2 := ?r1
-- after Shw ?r2 was already generalized.
test x = Tuple (fget x) (shw (fget x))

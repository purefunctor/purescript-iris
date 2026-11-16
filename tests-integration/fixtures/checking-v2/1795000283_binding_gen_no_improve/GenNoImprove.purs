module GenNoImprove where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

-- Control: the same classes, but no wanted is solved by a generalized given.
test x y = Tuple (fget x) (shw (fget y))

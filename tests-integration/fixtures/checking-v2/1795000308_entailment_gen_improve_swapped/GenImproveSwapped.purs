module GenImproveSwapped where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

test x = Tuple (shw (fget x)) (fget x)

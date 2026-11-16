module GenImproveClaim where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

test x = Tuple (fget x) (shw (fget x))

instance F Int String where
  fget _ = "not an int"

instance Shw Int where
  shw _ = "shw at Int"

-- Accepted: purs lets the caller pick the first component's type freely.
claim :: Tuple Int String
claim = test 1

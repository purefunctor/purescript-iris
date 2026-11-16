module GenImproveStr where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

fgetStr :: forall a. F a String => a -> String
fgetStr = fget

-- No Shw String instance. Shw ?r is generalized before ?r := String.
test x = Tuple (fgetStr x) (shw (fget x))

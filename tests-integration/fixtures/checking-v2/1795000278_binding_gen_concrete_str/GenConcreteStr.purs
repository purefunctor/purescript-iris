module GenConcreteStr where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

fgetStr :: forall a. F a String => a -> String
fgetStr = fget

-- Control for GenImproveStr: Shw String is concrete from the start.
test x = Tuple (fgetStr x) (shw (fgetStr x))

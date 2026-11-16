module GenImproveConcrete where

data Tuple a b = Tuple a b

class Shw a where
  shw :: a -> String

class F a b | a -> b where
  fget :: a -> b

instance Shw Int where
  shw _ = "shw at Int"

fgetInt :: forall a. F a Int => a -> Int
fgetInt = fget

-- Generalize pass: F ?x Int (generalized), Shw ?r (generalized),
-- then F ?x ?r is solved by the given F ?x Int and sets ?r := Int.
test x = Tuple (fgetInt x) (shw (fget x))

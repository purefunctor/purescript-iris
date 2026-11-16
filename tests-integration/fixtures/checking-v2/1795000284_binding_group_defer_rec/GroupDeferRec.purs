module GroupDeferRec where

class F a b | a -> b where
  fget :: a -> b

instance F Int String where
  fget _ = "not an int"

k :: forall a b. a -> b -> a
k a _ = a

asInt :: Int -> Int
asInt x = x

class Shw a where
  shw :: a -> String

instance Shw String where
  shw _ = "shw at String"

-- Shw ?r in z becomes Shw String once a solves F Int ?r.
z x = shw (a x)
a x = k (fget (asInt x)) z

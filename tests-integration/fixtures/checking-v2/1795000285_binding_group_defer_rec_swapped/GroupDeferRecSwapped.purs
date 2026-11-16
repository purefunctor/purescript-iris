module GroupDeferRecSwapped where

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

a x = k (fget (asInt x)) z
z x = shw (a x)

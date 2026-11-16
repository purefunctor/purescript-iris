module GroupStaleSwapped where

class F a b | a -> b where
  fget :: a -> b

instance F Int String where
  fget _ = "not an int"

k :: forall a b. a -> b -> a
k a _ = a

asInt :: Int -> Int
asInt x = x

a x = k (fget (asInt x)) z
z x = a x

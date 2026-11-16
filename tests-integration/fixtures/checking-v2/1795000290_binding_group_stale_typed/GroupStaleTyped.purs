module GroupStaleTyped where

class F a b | a -> b where
  fget :: a -> b

instance F Int String where
  fget _ = "not an int"

k :: forall a b. a -> b -> a
k a _ = a

asInt :: Int -> Int
asInt x = x

-- Control: z has a signature, so its type is not generalized from the group.
z :: Int -> String
z x = a x
a x = k (fget (asInt x)) z

module GroupStaleOrder where

class F a b | a -> b where
  fget :: a -> b

instance F Int String where
  fget _ = "not an int"

k :: forall a b. a -> b -> a
k x _ = x

asInt :: Int -> Int
asInt x = x

-- Control: the same group as GroupStale with the names swapped, so purs
-- processes the member holding F Int ?r (z) before the one it improves (a).
a x = z x
z x = k (fget (asInt x)) a

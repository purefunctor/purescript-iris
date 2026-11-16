module IrisGroupFreshSwapped where

k :: forall a b. a -> b -> a
k x _ = x

useInt :: Int -> Int
useInt x = x

-- z uses a's result as an Int; a returns a String. The group is ill-typed.
a x = k "str" z
z x = useInt (a x)

claim :: Int
claim = z 1

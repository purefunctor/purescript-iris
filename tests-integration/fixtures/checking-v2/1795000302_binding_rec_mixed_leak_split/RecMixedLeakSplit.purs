module RecMixedLeakSplit where

k :: forall a b. a -> b -> a
k x _ = x

-- Control: g does not mention f, so f and g are not one binding group.
f :: forall a. a -> a
f x = g x
g x = k x 0

useG :: Int
useG = g 1

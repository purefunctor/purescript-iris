module U9 where
-- Unknown head whose argument is a lambda, later applied to a higher-rank function.
g :: (forall a. a -> a) -> Int
g i = i 1
t = (\f -> f (\x -> x)) g

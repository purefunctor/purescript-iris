module U6 where
identity :: forall a. a -> a
identity x = x
g :: (forall a. a -> a) -> Int
g i = i 1
-- Unknown head, polymorphic variable argument, then the head becomes higher rank.
t = (\f -> f identity) g

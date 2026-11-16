module U6Arrow where
identity :: forall a. a -> a
identity x = x
app :: forall a b. (a -> b) -> a -> b
app h v = h v
g :: (forall a. a -> a) -> Int
g i = i 1
-- Same as U6, but the head is a known function whose argument type is an unknown.
t = (\f -> app f identity) g

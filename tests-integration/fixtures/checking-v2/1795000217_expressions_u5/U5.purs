module U5 where
identity :: forall a. a -> a
identity x = x
g :: { x :: forall a. a -> a } -> Int
g r = r.x 1
-- Unknown head; record field annotated with a polytype around a variable.
t = (\f -> f { x: (identity :: forall a. a -> a) }) g

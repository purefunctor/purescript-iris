module U5Lam where
g :: { x :: forall a. a -> a } -> Int
g r = r.x 1
-- Same, but the annotated field is a lambda.
t = (\f -> f { x: ((\y -> y) :: forall a. a -> a) }) g

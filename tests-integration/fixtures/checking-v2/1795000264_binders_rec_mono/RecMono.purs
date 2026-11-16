module RecMono where
-- BoxMono through a record binder: the field is polymorphic, the annotation is not.
h :: { x :: forall a. a -> a } -> Int
h { x: (g :: Int -> Int) } = g 1

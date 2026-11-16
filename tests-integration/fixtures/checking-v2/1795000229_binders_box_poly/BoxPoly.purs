module BoxPoly where
-- Control for BoxMono: the annotation is the field type.
data Box = Box (forall a. a -> a)
f :: Box -> Int
f (Box (g :: forall a. a -> a)) = g 1

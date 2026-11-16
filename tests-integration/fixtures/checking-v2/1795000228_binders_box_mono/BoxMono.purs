module BoxMono where
-- A binder annotated with a monotype receives a polymorphic field.
-- Using a forall a. a -> a at Int -> Int is safe; does the checker allow it?
data Box = Box (forall a. a -> a)
f :: Box -> Int
f (Box (g :: Int -> Int)) = g 1

module TypedPolyOk where
foreign import len :: String -> Int
-- Control: the annotation equals the argument type.
h :: { x :: forall a. a -> a } -> Int
h = \(r :: { x :: forall a. a -> a }) -> len (r.x "a")

module TypedMonoOk where
-- Control: a monotype annotation equal to the argument type.
h :: { x :: Int -> Int } -> Int
h = \(r :: { x :: Int -> Int }) -> r.x 1

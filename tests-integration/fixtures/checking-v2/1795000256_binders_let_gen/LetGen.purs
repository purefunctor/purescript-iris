module LetGen where
foreign import both :: Int -> String -> Int
-- A let-bound function without a signature, used at two types.
t :: Int
t = let f x = x in both (f 1) (f "a")

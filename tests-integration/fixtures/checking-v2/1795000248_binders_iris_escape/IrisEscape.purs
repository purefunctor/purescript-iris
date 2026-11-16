module IrisEscape where
data B = T | F
foreign import neg :: Int -> Int
foreign import len :: String -> Int
-- If a monomorphic { x :: Int -> Int } is accepted where { x :: forall a. a -> a }
-- is required, the T branch applies neg to a String.
k = case F of
  F -> \r -> r.x 2
  T -> \(r :: { x :: forall a. a -> a }) -> len (r.x "a")
pick :: B
pick = T
k2 = case pick of
  F -> \r -> r.x 2
  T -> \(r :: { x :: forall a. a -> a }) -> len (r.x "a")
result :: Int
result = k2 { x: neg }

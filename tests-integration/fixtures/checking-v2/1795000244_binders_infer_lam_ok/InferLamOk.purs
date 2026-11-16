module InferLamOk where
foreign import len :: String -> Int
foreign import idp :: forall a. a -> a
-- Control for IrisInferLam: the same lambda applied to a polymorphic argument.
k = \(f :: forall a. a -> a) -> len (f "a")
result :: Int
result = k idp

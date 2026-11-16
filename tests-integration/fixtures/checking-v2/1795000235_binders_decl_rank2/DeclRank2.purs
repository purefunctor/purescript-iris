module DeclRank2 where
foreign import both :: Int -> String -> Int
-- The equation form of InferLamOk: the polytype annotation is dropped, so f is monomorphic.
k (f :: forall a. a -> a) = both (f 1) (f "a")

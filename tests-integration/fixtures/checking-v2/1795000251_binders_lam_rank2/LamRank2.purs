module LamRank2 where
foreign import both :: Int -> String -> Int
-- Control for DeclRank2: the same function written as a lambda.
k = \(f :: forall a. a -> a) -> both (f 1) (f "a")

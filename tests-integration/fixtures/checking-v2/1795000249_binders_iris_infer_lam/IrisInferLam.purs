module IrisInferLam where
foreign import neg :: Int -> Int
foreign import len :: String -> Int
-- An inferred lambda with a polytype binder annotation, no signature anywhere.
-- purs infers the rank-2 type (forall a. a -> a) -> Int; passing neg must fail.
k = \(f :: forall a. a -> a) -> len (f "a")
result :: Int
result = k neg

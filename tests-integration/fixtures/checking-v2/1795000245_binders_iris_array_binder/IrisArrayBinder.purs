module IrisArrayBinder where
foreign import neg :: Int -> Int
foreign import len :: String -> Int
foreign import arr :: Array (Int -> Int)
-- A typed binder inside an array binder: each element binder is inferred and then
-- subtyped into the element type, so the annotation is instantiated.
result :: Int
result = case arr of
  [(f :: forall a. a -> a)] -> len (f "a")
  _ -> 0

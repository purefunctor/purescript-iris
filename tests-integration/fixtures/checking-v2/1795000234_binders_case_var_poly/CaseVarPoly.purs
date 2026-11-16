module CaseVarPoly where
foreign import idp :: forall a. a -> a
foreign import both :: Int -> String -> Int
-- A variable binder does not need a monotype, so purs keeps the scrutinee polymorphic.
t :: Int
t = case idp of
  f -> both (f 1) (f "a")

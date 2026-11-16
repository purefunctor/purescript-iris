module SynCase where
type P = forall a. a -> a
foreign import idp :: forall a. a -> a
foreign import both :: Int -> String -> Int
-- SynBinder as a case on a polymorphic scrutinee, using the binder at two types.
t :: Int
t = case idp of
  (f :: P) -> both (f 1) (f "a")

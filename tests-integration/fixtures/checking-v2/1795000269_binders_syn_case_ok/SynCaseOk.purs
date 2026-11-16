module SynCaseOk where
foreign import idp :: forall a. a -> a
foreign import both :: Int -> String -> Int
-- Control for SynCase: the polytype written out.
t :: Int
t = case idp of
  (f :: forall a. a -> a) -> both (f 1) (f "a")

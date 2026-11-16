module CaseMono where
foreign import idp :: forall a. a -> a
-- Control for GuardMono: the same binder as a case alternative.
t :: Int -> Int
t n = case idp of
  (g :: Int -> Int) -> g n

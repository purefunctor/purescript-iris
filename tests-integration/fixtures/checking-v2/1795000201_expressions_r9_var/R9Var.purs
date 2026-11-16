module R9Var where
data Tuple a b = Tuple a b
identity :: forall a. a -> a
identity x = x
-- Control for R9: an unannotated variable field is instantiated by both.
t = case { f: identity } of
  r -> Tuple (r.f 1) (r.f "a")

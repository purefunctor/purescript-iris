module R9 where
data Tuple a b = Tuple a b
identity :: forall a. a -> a
identity x = x
-- A record field annotated with a polytype keeps it in purs (the field is a
-- TypedValue, not a Var), so a case-bound r can use r.f at two types.
t = case { f: (identity :: forall a. a -> a) } of
  r -> Tuple (r.f 1) (r.f "a")

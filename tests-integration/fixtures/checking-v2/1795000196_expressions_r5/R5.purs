module R5 where
identity :: forall a. a -> a
identity x = x
-- Inferred type of a record whose field is a variable annotated with a polytype.
t :: Int
t = { f: (identity :: forall a. a -> a) }

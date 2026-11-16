module R8 where
identity :: forall a. a -> a
identity x = x
-- Array of records: first element keeps an annotated polytype field in purs only.
t = [{ f: (identity :: forall a. a -> a) }, { f: \x -> x }]

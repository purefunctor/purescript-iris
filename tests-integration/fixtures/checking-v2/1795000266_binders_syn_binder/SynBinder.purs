module SynBinder where
type P = forall a. a -> a
-- A binder annotated with a synonym for a polytype. isMonoType sees the synonym name.
h :: (forall a. a -> a) -> Int
h = \(f :: P) -> f 1

module SynBinderOk where
-- Control for SynBinder: the polytype written out.
h :: (forall a. a -> a) -> Int
h = \(f :: forall a. a -> a) -> f 1

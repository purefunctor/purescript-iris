module R1 where
identity :: forall a. a -> a
identity x = x
-- Record update checked against a record with a polymorphic field.
upd :: { f :: forall a. a -> a, n :: Int } -> { f :: forall a. a -> a, n :: Int }
upd r = r { f = identity }

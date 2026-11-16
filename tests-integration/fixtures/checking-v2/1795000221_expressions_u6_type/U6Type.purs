module U6Type where
identity :: forall a. a -> a
identity x = x
-- Expose the type each compiler gives the lambda in U6 by forcing a mismatch.
t :: Int
t = \f -> f identity

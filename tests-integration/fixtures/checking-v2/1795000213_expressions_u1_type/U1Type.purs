module U1Type where
-- Expose the type each compiler gives the argument in U1 by forcing a mismatch.
t :: Int
t = if true then { x: ((\y -> y) :: forall a. a -> a) } else { x: \y -> y }

module SubTailMissing where
foreign import x :: forall r. { a :: Record r | r }
-- Label a solves ?r := (), so x is { a :: {} } and label b is missing.
z :: { a :: {}, b :: Int }
z = x

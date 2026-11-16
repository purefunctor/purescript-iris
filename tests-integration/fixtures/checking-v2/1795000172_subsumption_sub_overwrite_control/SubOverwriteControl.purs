module SubOverwriteControl where
foreign import x :: forall r. { a :: Record r | r }
-- The well-typed neighbour of SubOverwrite: label a and the tail agree on b :: Int.
z :: { a :: { b :: Int }, b :: Int }
z = x

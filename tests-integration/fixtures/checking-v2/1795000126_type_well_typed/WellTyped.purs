module WellTyped where

-- Control for Overwrite: the field's row and the tail are different unknowns, so nothing is solved twice.
foreign import x :: forall r1 r2. { a :: Record r1 | r2 }

y :: { a :: { b :: Int }, b :: Int }
y = { a: { b: 1 }, b: 2 }

arr = [x, y]

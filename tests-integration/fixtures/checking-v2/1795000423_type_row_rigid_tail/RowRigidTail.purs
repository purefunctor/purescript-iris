module RowRigidTail where

-- A row with extra fields does not unify with a rigid row.
extra :: forall r. { a :: Int | r } -> Record r
extra x = x

-- A closed empty row does not unify with a rigid row.
closed :: forall r. {} -> Record r
closed x = x

-- Control: a rigid row unifies with itself.
same :: forall r. { a :: Int | r } -> { a :: Int | r }
same x = x

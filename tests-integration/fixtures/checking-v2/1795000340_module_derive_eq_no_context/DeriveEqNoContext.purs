module DeriveEqNoContext where

import Data.Eq (class Eq)

data T a = A a

-- The generated eq compares the `a` field, so it needs Eq a.
derive instance eqT :: Eq (T a)

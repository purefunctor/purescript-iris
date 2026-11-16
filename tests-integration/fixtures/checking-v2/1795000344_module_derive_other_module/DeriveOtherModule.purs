module DeriveOtherModule where

import Data.Eq (class Eq)

-- Boolean is defined in Prim, not in this module.
derive instance eqBoolean :: Eq Boolean

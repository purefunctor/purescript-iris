module DeriveNewtype where

import Data.Eq (class Eq, eq)

newtype W = W Int

-- The dictionary is Eq Int's, reused.
derive newtype instance eqW :: Eq W

t :: Boolean
t = eq (W 1) (W 1)

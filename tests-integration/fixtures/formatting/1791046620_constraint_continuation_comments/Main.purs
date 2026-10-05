-- @format width=20 indent=1
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

instance (Show value) -- instance explanation
  => Show (Box value)

derive instance (Eq value)
  -- derive explanation
  => Eq (Box value)

class (Eq value)
  -- superclass explanation
  <= Ord value

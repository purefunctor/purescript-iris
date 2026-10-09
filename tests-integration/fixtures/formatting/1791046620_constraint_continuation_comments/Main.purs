-- @format width=20 indent=1
-- @format width=30
-- @format width=80
-- @format width=40 indent=4 unicode=true
-- @format width=41 indent=4 unicode=true
-- @format width=42 indent=4 unicode=true
module Main where

instance (Show value) -- instance explanation
  => Show (Box value)

derive instance (Eq value)
  -- derive explanation
  => Eq (Box value)

class (Eq value, Show value)
  -- superclass explanation
  <= Ord value

class (Eq value, Show value) <= Thing value

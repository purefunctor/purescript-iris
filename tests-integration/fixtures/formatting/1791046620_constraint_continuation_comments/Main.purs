-- @format width=80
-- @format width=100 indent=4 unicode=true
module Main where

instance (Show value) -- explains why showing a box only requires showing the value inside it
  => Show (Box value)

derive instance (Eq value)
  -- derive explanation
  => Eq (Box value)

class (Eq value, Show value)
  -- superclass explanation
  <= Ord value

class (Eq element, Ord element, Show element, Enum element) <= Container element

class (Eq element, Ord element, Show element, Enum element) <= Collection element

-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

data Choice = First -- first constructor
  | Second

data Another
  -- equation explanation
  = Only

guarded value
  | first value -- first guard
  , second value = result

class C a b c | a -> b -- first dependency
  , b -> c

instance Show a -- explains why showing a box only requires showing the value inside it
  => Show (Box a)

arrow :: Int -- input explanation
  -> Int

-- @format width=80 incomplete=false
-- @format width=20
-- @format width=40 indent=4 unicode=true
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

instance Show a -- constraint explanation
  => Show (Box a)

arrow :: Int -- input explanation
  -> Int

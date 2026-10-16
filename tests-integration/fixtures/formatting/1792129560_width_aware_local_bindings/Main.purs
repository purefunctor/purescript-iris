-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

local value = do
  action value
  where
    helper = transform firstArgument secondArgument thirdArgument fourthArgument fifthArgument
localLet = let bound = firstArgument in transform bound secondArgument

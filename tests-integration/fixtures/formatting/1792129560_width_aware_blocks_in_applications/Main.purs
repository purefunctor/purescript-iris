-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

blockHead =
  do
      action
    argument
blockMiddle =
  use do
      action
    argument
blockFinal = use do
  action
brokenApplication = invokeOperation firstArgument secondArgument thirdArgument fourthArgument do
  action

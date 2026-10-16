-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

argument = use [ do
  action, nextArgument ] finalArgument

updateEffect = record { field = do
  action, otherField = secondArgument }
widePrefix = use "界界界界" { field: do
  action, otherField: secondArgument }

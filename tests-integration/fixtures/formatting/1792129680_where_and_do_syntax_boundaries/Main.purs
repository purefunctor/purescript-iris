-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

whereValue = result
  where
  result = 1
doValue = do
  value <- action
  value
doEndingInBind = do
  value <- action
doEndingInLet = do
  let value = 1
adoValue = ado
  value <- action
  in value

-- @format width=30
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

lambda = for_ values (\value -> do
  log value)

parenthesized = use (do
  action) next

record = { onClick: do
  action, other: 1 }

array = [ do
  firstAction, do
  secondAction ]

applicative = use (ado
  first <- firstAction
  second <- secondAction
  in combine first second)

wrapped = (combine firstArgument secondArgument)
nested = ((combine firstArgument secondArgument))

operator = identity $ { onClick: do
  action, other: 1 }

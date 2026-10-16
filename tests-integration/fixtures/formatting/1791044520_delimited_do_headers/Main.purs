-- @format width=80
-- @format width=100 indent=4 unicode=true
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

wrapped = (combine firstArgument secondArgument thirdArgument fourthArgument fifthArgument)
nested = ((combine firstArgument secondArgument thirdArgument fourthArgument fifthArgument))

operator = identity $ { onClick: do
  action, other: 1 }

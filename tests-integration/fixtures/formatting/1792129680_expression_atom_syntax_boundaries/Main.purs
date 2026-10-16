-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

parenthesized = (1)
negative = -1
typed = (1 :: Int)
typeArgument = identity @Int 1
operator = (1 + 2)
operatorName = (+)
section = _ + 1
infixValue = a `f` b
tickChain = a `f + g` b
record = { a: 1 }
recordPun = { value }
recordUpdate = record { a = 2, nested { field = 3 } }
empty = { record: {}, array: [] }
emptyAdo = ado in 1
hole = ?hole

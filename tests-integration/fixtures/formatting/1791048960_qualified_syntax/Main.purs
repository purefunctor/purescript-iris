-- @format width=20 incomplete=false
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

import A.B
import A.B (value, Type, (+)) as M

value = M.value
constructor = M.Constructor
signature :: M.Type
operator = M.(+)
chain = a M.+ b
tick = a `M.f` b
instance M.Show M.Type
infixl 6 M.add as +
qualifiedDo = M.do
  action
qualifiedAdo = M.ado
  value <- action
  in value

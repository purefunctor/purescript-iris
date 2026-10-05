-- @format width=20
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

value = firstLongOperand `combine (inner `f` argument)` secondLongOperand
nested = firstOperand `combine (inner `f (deeper `g` argument)` another)` secondOperand

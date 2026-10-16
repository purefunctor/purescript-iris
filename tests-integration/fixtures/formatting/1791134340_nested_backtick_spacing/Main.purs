-- @format width=80
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

value = firstOperandWithALongName `combineWith (innerOperandName `transformWith` innerArgumentName)` secondOperandWithALongName
nested = firstOperandWithALongName `combineWith (innerOperandName `transformWith (deeperOperandName `adjustWith` deeperArgumentName)` anotherArgumentName)` secondOperandWithALongName

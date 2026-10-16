-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

type Tail row = { | row }
type RowTail row = ( | row )
data Wrapped = Wrapped { firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType }

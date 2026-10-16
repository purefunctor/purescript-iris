-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

type Closed = { firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType }
type Open row = { firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType | row }
type Row row = ( firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType | row )

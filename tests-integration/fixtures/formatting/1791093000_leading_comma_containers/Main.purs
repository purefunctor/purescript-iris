-- @format width=24 indent=1 incomplete=false
-- @format width=40 indent=2
-- @format width=40 indent=4 unicode=true
-- @format width=120
module Main where

record = { firstField: firstArgument, secondField: secondArgument }
array = [ firstArgument, secondArgument, thirdArgument ]
nested = { outer: { left: firstArgument, right: secondArgument }, tail: [ firstArgument, secondArgument ] }
updated value = value { firstField = firstArgument, nested { left = firstArgument, right = secondArgument }, lastField = thirdArgument }
type Closed = { firstField :: FirstType, secondField :: SecondType }
type Open row = { firstField :: FirstType, secondField :: SecondType | row }
type Row row = ( firstField :: FirstType, secondField :: SecondType | row )
recordPattern { firstField: firstArgument, secondField: secondArgument } = firstArgument
arrayPattern [ firstArgument, secondArgument, thirdArgument ] = secondArgument
singletons = { one: [ firstArgument ], empty: {}, list: [] }

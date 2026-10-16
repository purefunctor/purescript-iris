-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

record = { firstField: firstArgument, secondField: secondArgument, thirdField: thirdArgument }
array = [ firstArgument, secondArgument, thirdArgument, fourthArgument, fifthArgument ]
nested = { outer: { left: firstArgument, middle: secondArgument, right: thirdArgument, last: fourthArgument }, tail: [ firstArgument, secondArgument ] }
updated value = value { firstField = firstArgument, nested { left = firstArgument, middle = secondArgument, right = thirdArgument, last = fourthArgument }, lastField = thirdArgument }
type Closed = { firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType }
type Open row = { firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType | row }
type Row row = ( firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType | row )
recordPattern { firstField: firstArgument, secondField: secondArgument, thirdField: thirdArgument } = firstArgument
arrayPattern [ firstArgument, secondArgument, thirdArgument, fourthArgument, fifthArgument ] = secondArgument
singletons = { singletonArray: [ firstArgument ], singletonRecord: { field: secondArgument }, emptyRecord: {}, emptyArray: [] }

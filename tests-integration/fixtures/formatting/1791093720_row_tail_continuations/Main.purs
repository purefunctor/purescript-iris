-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

type Tail = ( | VeryLongRowTypeConstructorName firstArgument secondArgument thirdArgument fourthArgument )
type Fields = ( field :: Int | VeryLongRowTypeConstructorName firstArgument secondArgument thirdArgument fourthArgument )
type RecordTail = { | VeryLongRowTypeConstructorName firstArgument secondArgument thirdArgument fourthArgument }
type CommentTail = ( field :: Int | -- tail comment
  VeryLongRowTypeConstructorName firstArgument secondArgument thirdArgument fourthArgument )

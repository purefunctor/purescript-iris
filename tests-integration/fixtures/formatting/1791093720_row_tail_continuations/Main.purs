-- @format width=20 indent=1 incomplete=false
-- @format width=40 indent=2
-- @format width=40 indent=4 unicode=true
-- @format width=120
module Main where

type Tail = ( | VeryLongRowTypeConstructorName firstArgument secondArgument )
type Fields = ( field :: Int | VeryLongRowTypeConstructorName firstArgument secondArgument )
type RecordTail = { | VeryLongRowTypeConstructorName firstArgument secondArgument }
type CommentTail = ( field :: Int | -- tail comment
  VeryLongRowTypeConstructorName firstArgument secondArgument )

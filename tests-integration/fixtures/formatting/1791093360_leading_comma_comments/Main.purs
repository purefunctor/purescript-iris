-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

array = [ -- opening comment
  firstArgument -- before comma
  , -- after comma
  secondArgument
  -- before closing
  ]

record = { {- opening block -} firstField: firstArgument
  -- comma comment
  , secondField: {- value block -} secondArgument
  -- closing comment
  }

block = [ {- opening
block -} firstArgument, secondArgument ]
type Row = ( {- field -} firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType )

value = result
  where -- keyword comment
  -- first binding comment
  result :: Int
  result = 1

guarded = result
  where
  result value
    | predicate value = firstArgument
    | otherwise = secondArgument
  sibling = 2

class Example a where
  example :: a -> Int
instance Example Int where
  example value = local
    where
    local = value

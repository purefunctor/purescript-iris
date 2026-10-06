-- @format width=30
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

plain value = result
  where
    result = value

guarded value
  | predicate value = first
  | otherwise = second
  where
    first = 1
    second = 2

monadic = do
  action
  where
    helper = 1

helperOnly = 1
  where
    helper = do
      action

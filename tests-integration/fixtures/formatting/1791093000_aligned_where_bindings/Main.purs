-- @format width=24 indent=1 incomplete=false
-- @format width=40 indent=2
-- @format width=40 indent=4 unicode=true
-- @format width=120
module Main where

value = combine first second
  where
  first = nested
    where
    nested = 1
  second = 2

effect = do
  consume local
  pure local
  where
  local = do
    action
    pure 1
  sibling = 2

branch input = case input of
  Just value -> result
    where
    result = value
  Nothing -> fallback
  where
  fallback = 0

guarded value
  | predicate value = result
  | otherwise = fallback
  where
  result = value
  fallback = 0

unrelated = let local = 1 in local

applicative = ado
  value <- action
  in local value
  where
  local value = value

continuation = use do
    action
    next
  argument
  where
  next = 1

inside = do
  let helper = result
        where
        Tuple result other = pair
  pure helper

-- @format width=30 indent=4
-- @format width=80
-- @format width=30 indent=4 unicode=true
module Main where

main = traverse_ (\item -> log ("processing " <> show item)) (Array.range 1 someUpperBoundValue)

terms = do
  (r.a)
  (_.a)
  (-1)
  (h @b)
  ({ a: 1 })
  ((a do
    b))

patterns value = case value of
  [-1] -> 0
  _ -> 1

rows :: forall r. { | r } -> { | r } -> Int
quantified :: (forall b. b -> b) -> Int

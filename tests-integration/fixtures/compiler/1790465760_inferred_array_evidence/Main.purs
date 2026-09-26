module Main where

data Choice = Chosen Int | Other

test = [\(Chosen value) -> value, \(Chosen _) -> 23]

test' :: Partial => Array (Choice -> Int)
test' = [\(Chosen value) -> value, \(Chosen _) -> 23]

class Extract a where
  extract :: a -> Int

instance Extract Choice where
  extract (Chosen value) = value
  extract Other = 41

test2 = [extract, \(Chosen _) -> 29]

-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

class C a
class D a b | -> a, a -> b
instance C Int
derive instance C
class Eq a <= Ordered a where
  compare :: a -> a -> Int
instance Show a => Show (Wrapped a) where
  show :: Wrapped a -> String
  show (Wrapped value) = show value

-- @format width=40 incomplete=false
-- @format width=80
-- @format width=20 indent=4 unicode=true
module Main where

doValue = do
  first

  second

caseValue = case value of
  First -> 1

  Second -> 2

whereValue = first
  where
  first = 1

  second = 2

letValue = let
  first = 1

  second = 2
  in first

class C a where
  first :: a

  second :: a

instance C Int where
  first = 1

  second = 2

literal = """first
  
second"""

{- first
  
second -}
commentValue = 1

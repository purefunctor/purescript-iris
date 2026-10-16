-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

type EmptyRow = ()
type EmptyRecord = {}
type OpenRow r = ( | r )
type OpenRecord r = { | r }
type RowFields r = ( field :: Int, other :: String | r )
type RecordFields r = { field :: Int, other :: String | r }
type Quantified = forall @a (b :: Type). a -> b
type Kinded = Int :: Type
type Signed = P -1
type Arrow = Int -> Int
type Constrained = Show a => a
type Operator = Int + Int

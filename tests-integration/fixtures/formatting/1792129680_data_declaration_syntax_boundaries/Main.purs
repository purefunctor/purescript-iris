-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

data Void
data Box (a :: Type) = Box a
newtype Wrapped a = Wrapped a
type role Wrapped representational

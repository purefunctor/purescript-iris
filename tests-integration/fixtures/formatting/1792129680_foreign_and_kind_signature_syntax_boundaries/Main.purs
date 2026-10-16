-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

foreign import external :: Int
foreign import data External :: Type
data Kind :: Type
newtype NewKind :: Type
type AliasKind :: Type
class ClassKind :: Type -> Constraint

module Main where

data Box :: Type
data Box a = Box a

type Pair :: Type -> Type
type Pair a b = { first :: a, second :: b }

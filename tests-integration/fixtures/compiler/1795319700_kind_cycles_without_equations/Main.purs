module Main where

class Cyclic :: (Cyclic Int => Type) -> Constraint
class Cyclic a

data Orphan :: Orphan -> Type

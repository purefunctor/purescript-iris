module Main where

class Single :: Type -> Constraint
class Single a

instance Single a

instance Single (Int String)

module Main where

import Data.Show (show)

class Convert :: Type -> Type -> Constraint
class Convert a b | a -> b where
  convert :: a -> b

instance Convert a Int where
  convert _ = 0

showsConverted :: Int String -> String
showsConverted value = show (convert value)

inferredConverted value = convert (value :: Int String)

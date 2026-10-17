module Main where

import Data.Show (show)

class Convert :: Type -> Type -> Constraint
class Convert a b | a -> b where
  convert :: a -> b

instance Convert Int String where
  convert _ = "int"

wantedShow :: Int String -> String
wantedShow value = show value

wantedShowArray :: Array (Int String) -> String
wantedShowArray value = show value

wantedDependency :: Int String -> Int
wantedDependency value = convert value

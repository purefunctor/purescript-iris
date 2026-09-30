module Main where

-- !

value :: Int -> Int
value 0 = 4
value x = x
value :: Int -> Int

foreign import mismatch :: Int
mismatch :: Int

data Box :: Type -> Type
data Box a = Box a
newtype Box a = Other a
type role Box representational

class Inspect a where
  inspect :: a -> Int

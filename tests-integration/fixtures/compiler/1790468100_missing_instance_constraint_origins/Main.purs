module Main where

import Data.Semiring (class Semiring, add)

test :: String
test = add "left" "right"

test' = add "left" "right"

test2 :: forall a. a -> a
test2 value = add value value

test2' value = add value value

test3 :: forall a. Semiring a => a -> a
test3 value = add value value

data Box a = Box a

class Combine a where
  combine :: a -> a -> a

instance Semiring a => Combine (Box a) where
  combine (Box left) (Box right) = Box (add left right)

test4 :: Box String
test4 = combine (Box "left") (Box "right")

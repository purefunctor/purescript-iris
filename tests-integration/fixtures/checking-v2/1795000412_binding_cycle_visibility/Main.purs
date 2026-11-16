module Main where

class Show a where
  show :: a -> String

class Bind m where
  bind :: forall a b. m a -> (a -> m b) -> m b

data Box a = Box a

instance Bind Box where
  bind (Box a) f = f a

parameters :: Int -> Int
parameters n = viaLambda n

viaLambda :: Int -> Int
viaLambda = \n -> parameters n

constrainedLeft :: forall a. Show a => a -> String
constrainedLeft = constrainedRight

constrainedRight :: forall a. Show a => a -> String
constrainedRight = constrainedLeft

boxed :: Box Int
boxed = do
  n <- Box 1
  _ <- boxed
  Box n

boxedImmediate :: Box Int
boxedImmediate = bind boxedImmediate Box

localGroup :: Int
localGroup =
  let
    first = second
    second = first
  in
    first

localLazy :: Int -> Int
localLazy =
  let
    go n = go n
  in
    go

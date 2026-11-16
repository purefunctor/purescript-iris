module Main where

class StringValue :: Type -> Constraint

instance StringValue String

foreign import color :: forall value. StringValue value => value -> Int

red :: Int
red = color "red"

missing :: Int
missing = color 0

module Main where

scoped :: forall a. a -> a
scoped x = go x
  where
  go :: a -> a
  go y = y

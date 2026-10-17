module Main where

class Describe a where
  describe :: a -> String

data Colour = Red | Green

instance Describe Colour where
  describe Red = "red"
  describe Green = "green"

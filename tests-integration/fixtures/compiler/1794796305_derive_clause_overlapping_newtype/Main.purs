module Main where

import Data.Newtype (class Newtype)

newtype Wrapper = Wrapper String
  derive (Newtype)

derive instance Newtype Wrapper _

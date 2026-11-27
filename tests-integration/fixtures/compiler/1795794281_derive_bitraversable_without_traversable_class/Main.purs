module Main where

import Data.Bitraversable (class Bitraversable)

data Wrap a b = Wrap (Array a) b

derive instance Bitraversable Wrap

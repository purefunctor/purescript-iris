module Main where

import Data.Bifoldable (class Bifoldable)

data Wrap a b = Wrap (Array a) b

derive instance Bifoldable Wrap

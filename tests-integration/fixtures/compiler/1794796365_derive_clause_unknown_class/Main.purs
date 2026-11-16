module Main where

import Prelude

data Foo = Foo
  derive (Eq, Missing)

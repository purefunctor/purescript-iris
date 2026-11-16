module Main where

import Prelude

data Box a = Box a
  derive (Functor)
type role Box representational

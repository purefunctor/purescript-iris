module Main where

class Marker a

data Box a = Box a
instance Marker (Box Int)
type role Box representational

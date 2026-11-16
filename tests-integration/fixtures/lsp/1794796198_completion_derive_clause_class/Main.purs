module Main where

class EqColour a

data Marker = EqMarker

data Colour = Red | Green
  derive (Eq)
--          ^

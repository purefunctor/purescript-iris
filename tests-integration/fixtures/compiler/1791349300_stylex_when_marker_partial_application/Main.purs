module Main where

import Iris.StyleX (ConditionalCase, Marker)
import Iris.StyleX.When as When

partialAncestorMarker :: Marker -> String -> ConditionalCase String
partialAncestorMarker = When.ancestorMarker ":hover"

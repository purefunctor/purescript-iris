module Main where

import Iris.StyleX as StyleX
import Iris.StyleX.When as When

rowMarker :: StyleX.Marker
rowMarker = StyleX.defineMarker

styles = StyleX.create
  { row:
      { color: StyleX.conditionalValue "blue"
          [ When.ancestorMarker ":hover" (StyleX.markerStyle rowMarker) "red" ]
      }
  }

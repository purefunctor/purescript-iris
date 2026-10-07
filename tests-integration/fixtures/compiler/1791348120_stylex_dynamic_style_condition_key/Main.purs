module Main where

import Iris.StyleX as StyleX
import Iris.StyleX.When as When

styles = StyleX.create
  { hovered: \selector -> { color: StyleX.conditionalValue "red" [ When.ancestor selector "blue" ] } }

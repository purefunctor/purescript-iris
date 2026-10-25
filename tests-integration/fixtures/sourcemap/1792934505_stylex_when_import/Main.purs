module Main (styles) where

import Iris.StyleX.When as When
import Iris.StyleX (Style)
import Iris.StyleX as StyleX

styles :: { root :: Style }
styles = StyleX.create
  { root: { color: StyleX.conditionalValue "blue" [ When.ancestor ":hover" "red" ] } }

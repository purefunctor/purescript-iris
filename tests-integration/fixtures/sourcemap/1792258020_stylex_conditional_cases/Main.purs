module Main where

import Iris.StyleX as StyleX
import Iris.StyleX.When as When

styles = StyleX.create
  { root:
      { padding: StyleX.conditionalValue 8
          [ StyleX.conditionalCase "@media print" 0
          , When.ancestor ":hover" 12
          ]
      }
  }

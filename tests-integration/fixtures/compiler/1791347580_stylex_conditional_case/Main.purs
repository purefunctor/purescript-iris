module Main where

import Iris.StyleX as StyleX
import Iris.StyleX.When as When
import Tokens (breakpoints)

print :: String
print = "@media print"

styles = StyleX.create
  { root:
      { padding: StyleX.conditionalValue 8
          [ StyleX.conditionalCase breakpoints.small 4
          , StyleX.conditionalCase print 0
          , StyleX.conditionalCase "@media (min-width: 1200px)" 16
          , When.ancestor ":hover" 12
          ]
      }
  }

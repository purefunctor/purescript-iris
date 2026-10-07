module Main where

import Iris.StyleX as StyleX

responsive :: String -> { root :: StyleX.Style }
responsive query = StyleX.create
  { root: { padding: StyleX.conditionalValue 8 [ StyleX.conditionalCase query 4 ] } }

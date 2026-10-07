module Main where

import Iris.StyleX as StyleX

nested :: String -> { root :: StyleX.Style }
nested colour = StyleX.create { root: { color: colour } }

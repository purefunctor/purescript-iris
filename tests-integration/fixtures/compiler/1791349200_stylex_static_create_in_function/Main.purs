module Main where

import Iris.StyleX as StyleX

nested :: String -> { root :: StyleX.Style }
nested _ = StyleX.create { root: { color: "red" } }

module Main where

import Iris.StyleX as StyleX

styles = StyleX.create { root: \(colour :: String) -> { color: colour } }

module Main where

import Iris.StyleX as StyleX

styles = StyleX.create { at: \x y -> { left: x, top: y } }

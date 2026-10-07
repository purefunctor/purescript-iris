module Main where

import Iris.StyleX as StyleX

styles = StyleX.create { fixed: \(_ :: Int) -> { width: 4 } }

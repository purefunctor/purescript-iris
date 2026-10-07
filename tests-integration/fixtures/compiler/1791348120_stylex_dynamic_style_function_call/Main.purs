module Main where

import Data.Show (show)
import Iris.StyleX as StyleX

styles = StyleX.create { sized: \(width :: Int) -> { width: show width } }

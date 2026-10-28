module Main where

import Prelude

import Iris.StyleX as StyleX

styles = StyleX.create
  { root: \(width :: Int) -> { width: width `div` 2, height: width `mod` 3 }
  }

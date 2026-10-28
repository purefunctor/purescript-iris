module Main where

import Prelude

import Iris.StyleX as StyleX

styles = StyleX.create
  { root: { padding: 17 `div` 4, margin: 17 `mod` 4 }
  }

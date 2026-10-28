module Main where

import Prelude

import Iris.StyleX as StyleX

fonts :: Array String
fonts = [ "Inter", "sans-serif" ]

styles = StyleX.create
  { root: { fontFamily: fonts <> [ "serif" ] }
  }

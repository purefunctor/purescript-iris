module Main where

import Data.Array (unsafeIndex)
import Iris.StyleX as StyleX
import Partial.Unsafe (unsafePartial)

spacing :: Array Int
spacing = [ 4, 8, 16 ]

styles = StyleX.create
  { root: { padding: unsafePartial (unsafeIndex spacing 1) }
  }

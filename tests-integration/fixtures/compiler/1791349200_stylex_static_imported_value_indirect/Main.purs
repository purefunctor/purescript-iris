module Main where

import Iris.StyleX as StyleX
import Tokens (ordinary)

colour = ordinary.accent

styles = StyleX.create { root: { color: colour } }

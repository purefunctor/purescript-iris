module Main (caption, styles) where

import Tokens (label)
import Tokens (variables)
import Iris.StyleX as StyleX

caption :: String
caption = label 3

styles = StyleX.create { root: { color: variables.accent } }

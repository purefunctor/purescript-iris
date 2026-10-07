module Main where

import Iris.StyleX as StyleX
import Tokens (alias)

styles = StyleX.create { root: { color: alias.accent } }

module Main where

import Iris.StyleX as StyleX
import Tokens (accent)

animation = StyleX.keyframes { from: { color: accent }, to: { color: "blue" } }

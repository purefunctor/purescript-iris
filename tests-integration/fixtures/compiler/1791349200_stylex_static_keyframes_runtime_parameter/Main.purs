module Main where

import Iris.StyleX as StyleX

nested :: Number -> StyleX.Keyframes
nested opacity = StyleX.keyframes { from: { opacity }, to: { opacity: 1.0 } }

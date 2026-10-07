module Main where

import Iris.StyleX as StyleX
import Tokens (accent)

transition = StyleX.viewTransitionClass { group: { color: accent } }

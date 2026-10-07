module Main where

import Iris.StyleX as StyleX
import Tokens (variables)

constants = StyleX.defineConsts { accent: variables.accent }

module Main where

import Iris.StyleX as StyleX
import Tokens (accent, variables)

theme = StyleX.createTheme variables { accent }

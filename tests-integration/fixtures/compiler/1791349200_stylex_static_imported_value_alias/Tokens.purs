module Tokens where

import Data.Function as Function
import Iris.StyleX as StyleX

accent :: String
accent = "red"

ordinary = { accent: "blue" }

variables = Function.apply StyleX.defineVars { accent: "navy" }

alias = variables

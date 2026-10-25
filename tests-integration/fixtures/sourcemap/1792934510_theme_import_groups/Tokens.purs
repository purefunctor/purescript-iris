module Tokens (label, variables) where

import Iris.StyleX as StyleX

label :: Int -> String
label _ = "accent"

variables = StyleX.defineVars { accent: "blue" }

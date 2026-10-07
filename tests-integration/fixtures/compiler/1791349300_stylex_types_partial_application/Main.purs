module Main where

import Iris.StyleX (TypedValue)
import Iris.StyleX.Types (Color)
import Iris.StyleX.Types as Types

partialColor :: String -> TypedValue Color String
partialColor = Types.color

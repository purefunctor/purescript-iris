module Main where

import Prelude
import Control.Extend as Extend

test :: String
test = Extend.extend (\f -> f "B") (\s -> s <> "!") "A"

test' = Extend.extend (\f -> f "B") (\s -> s <> "!") "A"

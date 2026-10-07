module Main where

import Iris.StyleX as StyleX

make :: String -> { sized :: Int -> StyleX.DynamicStyle }
make colour = StyleX.create { sized: \width -> { width, color: colour } }

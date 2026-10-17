module Main where

data Shape = Circle Int | Rectangle Int Int

width :: Shape -> Int
width (Circle radius) = radius
width (Rectangle horizontal _) = horizontal

unit :: Shape
unit = Circle 1

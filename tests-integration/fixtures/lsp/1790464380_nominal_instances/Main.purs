module Main where

import Shapes (class Area, area)

main :: Int
main = area 1

-- Every instance of a class, derived instances included.
-- nominal instances class Shapes.Area

-- Instances whose head mentions a type, including inside another type's arguments.
-- nominal instances type Shapes.Box
-- nominal instances type Shapes.Size

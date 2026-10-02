module Main where

import Lib (Box(..), increment, (<.>))

main :: Int
main = increment 1 <.> increment 2

boxed :: Box Int
boxed = Box main

-- Values, types, and constructors that share a name.
-- nominal signature Lib.increment
-- nominal signature Lib.Box

-- Operators, whose names may contain the dots that separate modules.
-- nominal signature Lib.<.>
-- nominal signature Lib.(<.>)

-- Re-exports, built-in modules, rejected declarations, and wrapped signatures.
-- nominal signature ReExport.increment
-- nominal signature Prim.Int
-- nominal signature Broken.<+>
-- nominal signature Lib.longKinded

-- Names that denote nothing.
-- nominal signature Lib.missing
-- nominal signature Missing.thing

-- nominal references Lib.increment
-- nominal references Lib.Box

-- Direct and indirect importers, each indirect one through a module it imports.
-- nominal dependents Lib

-- Whole names, prefixes, substrings, then letters in order, shown on one line.
-- nominal search box
-- nominal search long
-- nominal search icmt

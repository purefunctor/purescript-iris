module Main where

mainValue :: Int
mainValue = 42

data MainType = MainConstructor

class MainClass a

-- `İ` (U+0130) lowercases to `i` plus U+0307, so the folded name is longer
-- than the source name. A byte-length rejection must not apply here.
data İstanbul = İstanbulConstructor

-- #
-- # main
-- # MAIN
-- # mainv
-- # i̇stanbul
-- # i̇stanbulc
-- # library
-- # libraryc
-- # missing

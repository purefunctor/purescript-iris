module Main where

equal left right = left
plus left right = left

infix 4 equal as ==
infixl 4 plus as +.

leftThenNonAssociative :: Int
leftThenNonAssociative = 1 +. 2 == 3

nonAssociativeThenLeft :: Int
nonAssociativeThenLeft = 1 == 2 +. 3

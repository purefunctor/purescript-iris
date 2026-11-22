module Main where

equal left right = left
tilde left right = left

infix 4 equal as ==
infix 5 tilde as ~

lowerThenHigher :: Int
lowerThenHigher = 1 == 2 ~ 3

higherThenLower :: Int
higherThenLower = 1 ~ 2 == 3

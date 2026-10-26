module Main where

variables :: Int -> String -> String
variables = \x x -> x

pun :: Int -> { x :: String } -> String
pun = \x { x } -> x

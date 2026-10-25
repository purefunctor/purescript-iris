module Main where

foreign import greet :: String -> String

foreign import farewell :: String -> String

main :: String
main = greet "Iris"

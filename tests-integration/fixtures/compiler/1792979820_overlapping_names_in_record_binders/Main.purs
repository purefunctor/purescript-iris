module Main where

punAndArgument :: { x :: Int } -> Int -> Int
punAndArgument { x } x = x

punAndField :: { x :: Int, a :: Int } -> Int
punAndField { x, a: x } = x

fields :: { a :: Int, b :: Int } -> Int
fields { a: x, b: x } = x

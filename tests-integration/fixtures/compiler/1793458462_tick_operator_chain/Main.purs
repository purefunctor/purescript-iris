module Main where

import Prelude

function :: Int -> Int -> Int
function left _ = left

x :: Int -> Int
x value = value `function <<< identity` value

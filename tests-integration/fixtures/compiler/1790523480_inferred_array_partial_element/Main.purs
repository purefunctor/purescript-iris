module Main where

data Choice = Chosen Int | Other

fs = [\(Chosen value) -> value]

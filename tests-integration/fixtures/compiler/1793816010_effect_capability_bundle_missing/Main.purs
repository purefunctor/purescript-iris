module Main where

import Capabilities (First, inferred, reading)

missing :: Record (first :: First) -> Int
missing = reading

missingInferred :: Record (first :: First) -> Int
missingInferred = inferred

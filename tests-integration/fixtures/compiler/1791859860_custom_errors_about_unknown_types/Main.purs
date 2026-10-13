module Main where

import Prim.TypeError (class Fail, class Warn, Quote, Text)

type Invalid = Int String

failsWithUnknown :: Fail (Quote Invalid) => Int
failsWithUnknown = 0

usesFailsWithUnknown :: Int
usesFailsWithUnknown = failsWithUnknown

failsIndependently :: Fail (Text "independent failure") => Int
failsIndependently = 0

usesFailsIndependently :: Int
usesFailsIndependently = failsIndependently

warnsWithUnknown :: Warn (Quote Invalid) => Int
warnsWithUnknown = 0

usesWarnsWithUnknown :: Int
usesWarnsWithUnknown = warnsWithUnknown

warnsIndependently :: Warn (Text "independent warning") => Int
warnsIndependently = 0

usesWarnsIndependently :: Int
usesWarnsIndependently = warnsIndependently

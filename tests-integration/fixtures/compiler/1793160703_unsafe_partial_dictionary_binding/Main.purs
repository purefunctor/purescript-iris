module Main where

import Prelude

import Partial (crashWith)
import Partial.Unsafe (unsafePartial)

positive :: Partial => Int -> Int
positive value | value > 0 = value

unusedDictionary :: Int -> Int
unusedDictionary value = unsafePartial (value + 1)

forwardedDictionary :: Int -> Int
forwardedDictionary value = unsafePartial (positive value)

forwardedCrash :: Int -> Int
forwardedCrash value = unsafePartial (crashWith ("crashed with " <> show value))

extraArguments :: Int -> Int
extraArguments = unsafePartial (\left right -> positive left + right) 10

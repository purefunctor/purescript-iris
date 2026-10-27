module Main where

import Prelude

import Partial.Unsafe (unsafePartial)

positive :: Partial => Int -> Int
positive value | value > 0 = value

unsafePositive :: Int -> Int
unsafePositive value = unsafePartial (positive value)

unsafePositiveFunction :: Int -> Int
unsafePositiveFunction = unsafePartial positive

unsafePositiveApplied :: Int -> Int
unsafePositiveApplied value = unsafePartial positive value

unsafeLambda :: Int -> Int
unsafeLambda = unsafePartial \1 -> 1

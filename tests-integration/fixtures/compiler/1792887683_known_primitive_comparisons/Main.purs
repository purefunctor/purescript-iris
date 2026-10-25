module Main where

import Prelude

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

booleanEqual :: Boolean -> Boolean -> Boolean
booleanEqual left right = left == right

integerEqual :: Int -> Int -> Boolean
integerEqual left right = left == right

numberEqual :: Number -> Number -> Boolean
numberEqual left right = left == right

charNotEqual :: Char -> Char -> Boolean
charNotEqual left right = left /= right

stringNotEqual :: String -> String -> Boolean
stringNotEqual left right = left /= right

booleanLessThan :: Boolean -> Boolean -> Boolean
booleanLessThan left right = left < right

integerLessThanOrEqual :: Int -> Int -> Boolean
integerLessThanOrEqual left right = left <= right

charGreaterThan :: Char -> Char -> Boolean
charGreaterThan left right = left > right

stringGreaterThanOrEqual :: String -> String -> Boolean
stringGreaterThanOrEqual left right = left >= right

numberNotEqual :: Number -> Number -> Boolean
numberNotEqual left right = left /= right

numberLessThan :: Number -> Number -> Boolean
numberLessThan left right = left < right

numberLessThanOrEqual :: Number -> Number -> Boolean
numberLessThanOrEqual left right = left <= right

numberGreaterThan :: Number -> Number -> Boolean
numberGreaterThan left right = left > right

numberGreaterThanOrEqual :: Number -> Number -> Boolean
numberGreaterThanOrEqual left right = left >= right

numberNotGreaterThan :: Number -> Number -> Boolean
numberNotGreaterThan left right = not (left > right)

integerBetween :: Int -> Int -> Int -> Boolean
integerBetween low high value = low <= value && value < high

comparisonOrder :: Boolean -> Boolean
comparisonOrder _ = observe "left" 1 < observe "right" 2

comparisonCaseOrder :: Number -> Boolean -> Boolean
comparisonCaseOrder left flag = observe "left" left > case observe "flag" flag of
  true -> observe "right" 1.0
  false -> observe "right" 2.0

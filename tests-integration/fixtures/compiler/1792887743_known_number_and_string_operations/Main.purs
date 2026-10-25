module Main where

import Prelude

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

numberAdd :: Number -> Number -> Number
numberAdd left right = left + right

numberSubtract :: Number -> Number -> Number
numberSubtract left right = left - right

numberMultiply :: Number -> Number -> Number
numberMultiply left right = left * right

numberDivide :: Number -> Number -> Number
numberDivide left right = left / right

numberSubtractGrouped :: Number -> Number -> Number -> Number
numberSubtractGrouped first second third = first - (second - third)

numberAddGrouped :: Number -> Number -> Number -> Number
numberAddGrouped first second third = first + (second + third)

stringAppend :: String -> String -> String
stringAppend left right = left <> right

stringAppendGrouped :: String -> String -> String -> String
stringAppendGrouped first second third = first <> second <> third

numberAddOrder :: Boolean -> Number
numberAddOrder _ = observe "left" 20.0 + observe "right" 22.0

partiallyAppliedDivide :: Number -> Number
partiallyAppliedDivide = div 1.0

partiallyAppliedAppend :: String -> String
partiallyAppliedAppend = append "prefix"

integerDivide :: Int -> Int -> Int
integerDivide left right = left / right

arrayAppend :: Array Int -> Array Int -> Array Int
arrayAppend left right = left <> right

genericAppend :: forall value. Semigroup value => value -> value -> value
genericAppend left right = left <> right

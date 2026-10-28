module Main where

import Prelude

import Partial (crashWith)
import Partial.Unsafe (unsafePartial)

foreign import traced :: forall a. String -> a -> a

tracedValue :: Int -> Int
tracedValue value = traced "after argument" (unsafePartial (traced "inside" value))

tracedCrash :: Int -> Int
tracedCrash value = unsafePartial (crashWith (traced "message" "crashed")) + traced "unreached" value

tracedFunction :: Int -> Int
tracedFunction = unsafePartial (traced "function" \value -> traced "body" value)

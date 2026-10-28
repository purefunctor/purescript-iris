module Main where

import Data.Function.Uncurried (runFn2)
import Record.Unsafe.Union (unsafeUnion, unsafeUnionFn)

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

withDefaults :: { y :: String } -> { x :: Int, y :: String }
withDefaults options = unsafeUnion options { x: 1, y: "default" }

override :: { x :: Int, y :: String } -> { x :: Int, y :: String }
override record = runFn2 unsafeUnionFn { x: 0 } record

literals :: { x :: Int, y :: String }
literals = runFn2 unsafeUnionFn { x: 1 } { y: "right", x: 2 }

records :: { x :: Int } -> { y :: String } -> { x :: Int, y :: String }
records left right = runFn2 unsafeUnionFn left right

overrideOrder :: { x :: Int, y :: String } -> { x :: Int, y :: String }
overrideOrder record = unsafeUnion { x: observe "override" 0 } (observe "record" record)

literalsOrder :: Boolean -> { x :: Int, y :: String }
literalsOrder _ = runFn2 unsafeUnionFn { x: observe "left" 1 } { y: "right", x: observe "right" 2 }

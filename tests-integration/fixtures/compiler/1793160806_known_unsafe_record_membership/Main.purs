module Main where

import Record.Unsafe (unsafeHas)

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

present :: Boolean
present = unsafeHas "x" { x: 1, y: "y" }

absent :: Boolean
absent = unsafeHas "z" { x: 1, y: "y" }

presentOrder :: Boolean -> Boolean
presentOrder _ = unsafeHas "y" { x: observe "x" 1, y: observe "y" "y" }

unknown :: { x :: Int } -> Boolean
unknown record = unsafeHas "x" record

module Main where

import Record.Unsafe (unsafeDelete, unsafeGet, unsafeSet)

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

get :: { x :: Int, y :: String } -> Int
get record = unsafeGet "x" record

getQuoted :: { "first name" :: String } -> String
getQuoted record = unsafeGet "first name" record

set :: { x :: Int, y :: String } -> { x :: Int, y :: String }
set record = unsafeSet "x" 2 record

setNew :: { y :: String } -> { y :: String, x :: Int }
setNew record = unsafeSet "x" 2 record

delete :: { x :: Int, y :: String } -> { y :: String }
delete record = unsafeDelete "x" record

deleteLiteral :: Boolean -> { y :: String }
deleteLiteral _ = unsafeDelete "x" { x: observe "x" 1, y: observe "y" "y" }

setOrder :: { x :: Int, y :: String } -> { x :: Int, y :: String }
setOrder record = unsafeSet "x" (observe "value" 2) (observe "record" record)

deleteOrder :: { x :: Int, y :: String } -> { first :: Int, rest :: { y :: String } }
deleteOrder record =
  { first: observe "first" 1, rest: unsafeDelete "x" (observe "record" record) }

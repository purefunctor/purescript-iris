module Main where

import Record as Record
import Type.Proxy (Proxy(..))

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

delete :: { x :: Int, y :: String } -> { y :: String }
delete record = Record.delete (Proxy :: _ "x") record

deleteQuoted :: { "first name" :: String, y :: String } -> { y :: String }
deleteQuoted record = Record.delete (Proxy :: _ "first name") record

deleteOrder :: { x :: Int, y :: String } -> { first :: Int, rest :: { y :: String } }
deleteOrder record =
  { first: observe "first" 1, rest: Record.delete (Proxy :: _ "x") (observe "record" record) }

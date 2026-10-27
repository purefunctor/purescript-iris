module Main where

import Record as Record
import Type.Proxy (Proxy(..))

foreign import observe :: forall value. String -> value -> value

delete :: { x :: Int, y :: String } -> { y :: String }
delete record = Record.delete (Proxy :: _ "x") record

modify :: { x :: Int, y :: String } -> { x :: Int, y :: String }
modify record =
  Record.modify (Proxy :: _ "x") (observe "function" (\x -> x)) (observe "record" record)

withDefaults :: { y :: String } -> { x :: Int, y :: String }
withDefaults options = Record.merge options { x: 1, y: "default" }

module Main where

import Prelude

import Data.Symbol (class IsSymbol)
import Prim.Row as Row
import Record as Record
import Type.Proxy (Proxy(..))

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

get :: { x :: Int, y :: String } -> Int
get record = Record.get (Proxy :: _ "x") record

getQuoted :: { "first name" :: String } -> String
getQuoted record = Record.get (Proxy :: _ "first name") record

set :: { x :: Int, y :: String } -> { x :: String, y :: String }
set record = Record.set (Proxy :: _ "x") "set" record

modify :: { x :: Int, y :: String } -> { x :: Int, y :: String }
modify record = Record.modify (Proxy :: _ "x") (_ + 1) record

insert :: { y :: String } -> { x :: Int, y :: String }
insert record = Record.insert (Proxy :: _ "x") 1 record

setOrder :: { x :: Int, y :: String } -> { x :: String, y :: String }
setOrder record = Record.set (Proxy :: _ "x") (observe "value" "set") (observe "record" record)

modifyOrder :: { x :: Int, y :: String } -> { x :: Int, y :: String }
modifyOrder record =
  Record.modify (Proxy :: _ "x") (observe "function" (_ + 1)) (observe "record" record)

getPolymorphic
  :: forall label value rest row
   . IsSymbol label
  => Row.Cons label value rest row
  => Proxy label
  -> Record row
  -> value
getPolymorphic label record = Record.get label record

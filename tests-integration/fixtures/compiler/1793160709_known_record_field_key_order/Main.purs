module Main where

import Prelude

import Record as Record
import Type.Proxy (Proxy(..))

set :: { x :: Int, y :: String } -> { x :: String, y :: String }
set record = Record.set (Proxy :: _ "x") "set" record

setLiteral :: String -> { x :: String, y :: String }
setLiteral y = Record.set (Proxy :: _ "x") "set" { y, x: 0 }

insert :: { y :: String } -> { x :: Int, y :: String }
insert record = Record.insert (Proxy :: _ "x") 1 record

insertLiteral :: String -> { x :: Int, y :: String, z :: Int }
insertLiteral y = Record.insert (Proxy :: _ "x") 1 { z: 2, y }

modify :: { x :: Int, y :: String } -> { x :: Int, y :: String }
modify record = Record.modify (Proxy :: _ "x") (_ + 1) record

modifyLiteral :: String -> { x :: Int, y :: String }
modifyLiteral y = Record.modify (Proxy :: _ "x") (_ + 1) { y, x: 0 }

delete :: { x :: Int, y :: String, z :: Int } -> { y :: String, z :: Int }
delete record = Record.delete (Proxy :: _ "x") record

deleteLiteral :: String -> { y :: String, z :: Int }
deleteLiteral y = Record.delete (Proxy :: _ "x") { z: 2, x: 0, y }

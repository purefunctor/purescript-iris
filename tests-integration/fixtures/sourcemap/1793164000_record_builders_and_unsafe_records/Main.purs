module Main where

import Prelude

import Record.Builder as Builder
import Record.Unsafe (unsafeGet, unsafeHas)
import Type.Proxy (Proxy(..))

foreign import observe :: forall value. String -> value -> value

build :: { x :: Int, y :: String } -> { w :: Int, z :: Boolean }
build record =
  Builder.build
    ( Builder.insert (Proxy :: _ "z") (observe "z" true)
        >>> Builder.modify (Proxy :: _ "x") (\x -> x + 1)
        >>> Builder.delete (Proxy :: _ "y")
        >>> Builder.rename (Proxy :: _ "x") (Proxy :: _ "w")
    )
    record

fromScratch :: { x :: Int, y :: String }
fromScratch =
  Builder.buildFromScratch (Builder.insert (Proxy :: _ "x") 1 >>> Builder.insert (Proxy :: _ "y") "y")

get :: { x :: Int } -> Int
get record = unsafeGet "x" record

has :: Boolean
has = unsafeHas "x" { x: observe "x" 1 }

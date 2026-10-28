module Main where

import Prelude

import Record.Builder (Builder)
import Record.Builder as Builder
import Type.Proxy (Proxy(..))

insertFlipped :: { x :: Int } -> { x :: Int, y :: String, z :: Boolean }
insertFlipped record =
  Builder.build (Builder.insert (Proxy :: _ "y") "y" >>> Builder.insert (Proxy :: _ "z") true) record

insertComposed :: { x :: Int } -> { x :: Int, y :: String, z :: Boolean }
insertComposed record =
  Builder.build (Builder.insert (Proxy :: _ "z") true <<< Builder.insert (Proxy :: _ "y") "y") record

modify :: { x :: Int, y :: String } -> { x :: Int, y :: String }
modify record = Builder.build (Builder.modify (Proxy :: _ "x") (\x -> x + 1)) record

delete :: { x :: Int, y :: String } -> { y :: String }
delete record = Builder.build (Builder.delete (Proxy :: _ "x")) record

rename :: { x :: Int, y :: String } -> { y :: String, z :: Int }
rename record = Builder.build (Builder.rename (Proxy :: _ "x") (Proxy :: _ "z")) record

pipeline :: { x :: Int, y :: String } -> { w :: Int, z :: Boolean }
pipeline record =
  Builder.build
    ( Builder.insert (Proxy :: _ "z") true
        >>> Builder.modify (Proxy :: _ "x") (\x -> x * 2)
        >>> identity
        >>> Builder.delete (Proxy :: _ "y")
        >>> Builder.rename (Proxy :: _ "x") (Proxy :: _ "w")
        >>> Builder.nub
    )
    record

fromScratch :: { x :: Int, y :: String }
fromScratch =
  Builder.buildFromScratch (Builder.insert (Proxy :: _ "x") 1 >>> Builder.insert (Proxy :: _ "y") "y")

literal :: { w :: Int, z :: Boolean }
literal =
  Builder.build
    ( Builder.insert (Proxy :: _ "z") true
        >>> Builder.modify (Proxy :: _ "x") (\x -> x * 2)
        >>> Builder.delete (Proxy :: _ "y")
        >>> Builder.rename (Proxy :: _ "x") (Proxy :: _ "w")
    )
    { x: 1, y: "y" }

unknown :: Builder { x :: Int } { x :: Int, y :: String } -> { x :: Int } -> { x :: Int, y :: String }
unknown builder record = Builder.build builder record

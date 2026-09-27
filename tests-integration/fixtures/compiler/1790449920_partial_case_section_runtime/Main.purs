module Main where

import Data.Functor ((<#>))
import Effect (Effect)
import Partial.Unsafe (unsafePartial)

data BinaryType = Blob | ArrayBuffer

test :: Effect String -> Effect BinaryType
test source = unsafePartial do
  source <#> case _ of
    "blob" -> Blob
    "arraybuffer" -> ArrayBuffer

test' value = case value of
  "blob" -> Blob
  "arraybuffer" -> ArrayBuffer

test2 = \Blob -> 17

test3 :: String -> BinaryType
test3 value = unsafePartial (test' value)

test4 :: BinaryType -> Int
test4 value = unsafePartial (test2 value)

test5 :: Effect BinaryType -> Effect Int
test5 source = unsafePartial do
  source <#> \Blob -> 17

test6 :: BinaryType -> Int
test6 value = unsafePartial ((\Blob -> 23) value)

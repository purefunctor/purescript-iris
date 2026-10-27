module Main where

import Prelude

import Foreign.Object (Object)
import Foreign.Object as Object
import Foreign.Object.Unsafe as ObjectUnsafe

member :: String -> Object Int -> Boolean
member key object = Object.member key object

keys :: Object Int -> Array String
keys object = Object.keys object

index :: String -> Object Int -> Int
index key object = ObjectUnsafe.unsafeIndex object key

indexLiteral :: Object Int -> Int
indexLiteral object = ObjectUnsafe.unsafeIndex object "a" + ObjectUnsafe.unsafeIndex object "b-c"

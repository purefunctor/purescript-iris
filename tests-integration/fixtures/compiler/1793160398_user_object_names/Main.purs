module Main (module Library, keys, marker, member, sample, wrap) where

import Prelude

import Foreign.Object as Object
import Library (Value(..), describe, objectSize)
import Object as UserObject

sample :: Value
sample = Object (Object.insert "b" 2 (Object.singleton "a" 1))

wrap :: Object.Object Int -> Value
wrap = Object

keys :: Object.Object Int -> Array String
keys = \object -> let keysOfObject = Object.keys object in keysOfObject

member :: String -> Object.Object Int -> Boolean
member key object = Object.member key object

marker :: Int
marker = UserObject.marker

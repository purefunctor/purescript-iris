module Library (Value(..), describe, objectSize) where

import Prelude

import Data.Array as Array
import Foreign.Object as Object
import Partial.Unsafe (unsafePartial)

data Value = Object (Object.Object Int) | Array (Array Int) | Error String

describe :: Value -> Array String
describe value = case value of
  Object object | Object.member "a" object -> Object.keys object
  Object _ -> []
  Array [ first, second ] -> [ show (first + second) ]
  Array _ -> []
  Error message -> [ message ]

objectSize :: Value -> Int
objectSize value = unsafePartial case value of
  Object object -> Array.length (Object.keys object)

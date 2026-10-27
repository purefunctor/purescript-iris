module Main where

import Prelude

import Control.Monad.ST as ST
import Data.Array.ST as STArray
import Foreign.Object (Object)
import Foreign.Object as Object
import Foreign.Object.ST as STObject
import Foreign.Object.Unsafe as ObjectUnsafe

appended :: Array Int -> Array Int
appended values = ST.run do
  array <- STArray.thaw values
  _ <- STArray.push 0 array
  _ <- STArray.unshiftAll values array
  STArray.freeze array

inserted :: String -> Object Int -> Object Int
inserted key object = Object.runST do
  mutable <- Object.thawST object
  _ <- STObject.delete "removed" mutable
  STObject.poke key 1 mutable

described :: String -> Object Int -> Array String
described key object =
  if Object.member key object then Object.keys object
  else [ show (ObjectUnsafe.unsafeIndex object "fallback") ]

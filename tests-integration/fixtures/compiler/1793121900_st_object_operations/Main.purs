module Main where

import Prelude

import Control.Monad.ST as ST
import Foreign.Object (Object)
import Foreign.Object as Object
import Foreign.Object.ST as STObject

built :: String -> Object Int
built key = Object.runST do
  object <- STObject.new
  _ <- STObject.poke "a" 1 object
  poked <- STObject.poke key 2 object
  _ <- STObject.poke "removed" 3 poked
  STObject.delete "removed" poked

type Copies =
  { original :: Object Int
  , frozen :: Object Int
  , final :: Object Int
  }

copies :: Object Int -> Copies
copies original = ST.run do
  object <- Object.thawST original
  _ <- STObject.poke "b" 2 object
  frozen <- Object.freezeST object
  _ <- STObject.delete "a" object
  final <- Object.freezeST object
  pure { original, frozen, final }

module Iris.Effect.Json
  ( Json
  , parse
  , stringify
  , null
  , fromString
  , fromNumber
  , fromInt
  , fromBoolean
  , fromArray
  , fromObject
  , toString
  , toNumber
  , toInt
  , toBoolean
  , toArray
  , field
  ) where

import Data.Either (Either(..))
import Data.Maybe (Maybe(..))

foreign import data Json :: Type

parse :: String -> Either String Json
parse = parseWith Left Right

foreign import stringify :: Json -> String

foreign import null :: Json

foreign import fromString :: String -> Json

foreign import fromNumber :: Number -> Json

foreign import fromInt :: Int -> Json

foreign import fromBoolean :: Boolean -> Json

foreign import fromArray :: Array Json -> Json

foreign import fromObject :: Array { name :: String, value :: Json } -> Json

toString :: Json -> Maybe String
toString = toStringWith Just Nothing

toNumber :: Json -> Maybe Number
toNumber = toNumberWith Just Nothing

toInt :: Json -> Maybe Int
toInt = toIntWith Just Nothing

toBoolean :: Json -> Maybe Boolean
toBoolean = toBooleanWith Just Nothing

toArray :: Json -> Maybe (Array Json)
toArray = toArrayWith Just Nothing

field :: String -> Json -> Maybe Json
field = fieldWith Just Nothing

foreign import parseWith ::
  (String -> Either String Json) -> (Json -> Either String Json) -> String -> Either String Json

foreign import toStringWith :: (String -> Maybe String) -> Maybe String -> Json -> Maybe String

foreign import toNumberWith :: (Number -> Maybe Number) -> Maybe Number -> Json -> Maybe Number

foreign import toIntWith :: (Int -> Maybe Int) -> Maybe Int -> Json -> Maybe Int

foreign import toBooleanWith ::
  (Boolean -> Maybe Boolean) -> Maybe Boolean -> Json -> Maybe Boolean

foreign import toArrayWith ::
  (Array Json -> Maybe (Array Json)) -> Maybe (Array Json) -> Json -> Maybe (Array Json)

foreign import fieldWith :: (Json -> Maybe Json) -> Maybe Json -> String -> Json -> Maybe Json

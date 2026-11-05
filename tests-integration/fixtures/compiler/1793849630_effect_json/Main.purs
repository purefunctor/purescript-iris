module Main (results) where

import Prelude

import Data.Either (Either(..), either)
import Data.Maybe (Maybe(..))
import Iris.Effect.Json (Json)
import Iris.Effect.Json as Json

parsedInt :: String -> Maybe Int
parsedInt text = either (const Nothing) Json.toInt (Json.parse text)

prototypeField :: Json
prototypeField = Json.fromObject [ { name: "__proto__", value: Json.fromString "data" } ]

order :: Json
order = Json.fromObject
  [ { name: "product", value: Json.fromString "p1" }
  , { name: "quantity", value: Json.fromInt 2 }
  , { name: "gift", value: Json.fromBoolean false }
  , { name: "tags", value: Json.fromArray [ Json.fromString "new", Json.null ] }
  , { name: "price", value: Json.fromNumber 2.5 }
  ]

describeParse :: String -> String
describeParse text = case Json.parse text of
  Left _ -> "invalid"
  Right json -> case Json.field "product" json >>= Json.toString of
    Just product -> "product " <> product
    Nothing -> "no product"

results :: Array String
results =
  [ Json.stringify order
  , show (map parsedInt [ "2147483647", "2147483648", "-2147483648", "-2147483649", "1.5" ])
  , Json.stringify prototypeField
  , show (Json.field "__proto__" prototypeField >>= Json.toString)
  , show (Json.field "__proto__" (Json.fromObject []) >>= Json.toString)
  , show (Json.field "toString" (Json.fromObject []) >>= Json.toString)
  , show (Json.field "length" (Json.fromArray []) >>= Json.toInt)
  , show (map (map Json.stringify) (Json.field "tags" order >>= Json.toArray))
  , show (Json.field "gift" order >>= Json.toBoolean)
  , show (Json.field "price" order >>= Json.toNumber)
  , show (Json.field "price" order >>= Json.toInt)
  , describeParse (Json.stringify order)
  , describeParse "[1, 2]"
  , describeParse "{"
  ]

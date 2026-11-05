module AppError (AppError(..), status, toJson) where

import Prelude

import Iris.Effect.Json (Json)
import Iris.Effect.Json as Json

data AppError
  = NotFound String
  | BadRequest String
  | OutOfStock { product :: String, requested :: Int, available :: Int }
  | UpstreamFailed String

status :: AppError -> Int
status = case _ of
  NotFound _ -> 404
  BadRequest _ -> 400
  OutOfStock _ -> 409
  UpstreamFailed _ -> 502

toJson :: AppError -> Json
toJson = case _ of
  NotFound what -> message "not_found" (what <> " not found")
  BadRequest reason -> message "bad_request" reason
  OutOfStock { product, requested, available } -> Json.fromObject
    [ { name: "error", value: Json.fromString "out_of_stock" }
    , { name: "product", value: Json.fromString product }
    , { name: "requested", value: Json.fromInt requested }
    , { name: "available", value: Json.fromInt available }
    ]
  UpstreamFailed reason -> message "upstream_failed" reason

message :: String -> String -> Json
message kind text = Json.fromObject
  [ { name: "error", value: Json.fromString kind }
  , { name: "message", value: Json.fromString text }
  ]

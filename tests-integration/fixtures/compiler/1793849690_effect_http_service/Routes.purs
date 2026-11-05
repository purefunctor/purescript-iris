module Routes (handle, errorResponse) where

import Prelude

import AppError (AppError(..))
import AppError as AppError
import Data.Either (Either(..))
import Effect (Effect)
import Handlers (Shop)
import Handlers as Handlers
import Iris.Effect (class Has, type (&))
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Json (Json)
import Iris.Effect.Json as Json
import Logger (Logger)
import Logger as Logger
import Test.HttpServer (ServerRequest, ServerResponse)

foreign import segments :: String -> Array String

foreign import defect :: String -> Effect Unit

handle ::
  forall services.
  Has (Shop & Logger) services =>
  ServerRequest ->
  Async AppError services ServerResponse
handle request = do
  outcome <- Async.attempt (route request)
  let
    response = case outcome of
      Right success -> success
      Left error -> errorResponse error
  Async.sync (Logger.info (request.method <> " " <> request.path <> " " <> show response.status))
  pure response

route ::
  forall services.
  Has Shop services =>
  ServerRequest ->
  Async AppError services ServerResponse
route request = case request.method, segments request.path of
  "GET", [ "health" ] ->
    pure (json 200 (Json.fromObject [ { name: "status", value: Json.fromString "ok" } ]))
  "GET", [ "crash" ] -> do
    Async.liftEffect (defect "route crashed")
    pure (json 200 (Json.fromObject []))
  "GET", [ "products", identifier ] ->
    json 200 <<< Handlers.encodeProduct <$> Handlers.getProduct identifier
  "POST", [ "orders" ] -> json 201 <<< Handlers.encodeOrder <$> Handlers.createOrder request.body
  "GET", [ "orders", number ] -> json 200 <<< Handlers.encodeOrder <$> Handlers.getOrder number
  method, _ -> Async.fail (NotFound ("route " <> method <> " " <> request.path))

errorResponse :: AppError -> ServerResponse
errorResponse error = json (AppError.status error) (AppError.toJson error)

json :: Int -> Json -> ServerResponse
json status body =
  { status
  , headers: [ { name: "content-type", value: "application/json" } ]
  , body: Json.stringify body
  }

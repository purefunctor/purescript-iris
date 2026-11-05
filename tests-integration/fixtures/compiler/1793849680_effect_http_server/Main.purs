module Main (run) where

import Prelude

import Data.Array as Array
import Data.Either (Either(..))
import Data.Maybe (Maybe(..), fromMaybe)
import Data.String (joinWith)
import Effect (Effect)
import Iris.Effect (class Has, type (&))
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Console (Console)
import Iris.Effect.Console as Console
import Iris.Effect.Http (Header, Http, HttpError(..), Request)
import Iris.Effect.Http as Http
import Iris.Effect.Http.Node as HttpNode
import Iris.Effect.Json as Json
import Iris.Effect.Sync as Sync
import Signal (Signal)
import Signal as Signal
import Test.HttpServer (HttpServer, ServerError, ServerRequest, ServerResponse)
import Test.HttpServer as HttpServer

foreign import defect :: String -> Effect Unit

newtype Probe = Probe { started :: Signal, released :: Signal }

data RouteError
  = MissingName
  | UnknownRoute String

data ProgramError
  = ServerFailed ServerError
  | ClientFailed HttpError

instance showProgramError :: Show ProgramError where
  show = case _ of
    ServerFailed error -> "(ServerFailed " <> show error <> ")"
    ClientFailed error -> "(ClientFailed " <> show error <> ")"

text :: Int -> String -> ServerResponse
text status body = { status, headers: [ { name: "content-type", value: "text/plain" } ], body }

onError :: RouteError -> ServerResponse
onError = case _ of
  MissingName -> text 400 "missing name"
  UnknownRoute path -> text 404 ("unknown route " <> path)

header :: String -> Array Header -> Maybe String
header name headers = _.value <$> Array.find (\entry -> entry.name == name) headers

greeting :: forall services. ServerRequest -> Async RouteError services ServerResponse
greeting request = case Json.parse request.body of
  Right json | Just name <- Json.field "name" json >>= Json.toString ->
    pure
      { status: 200
      , headers: [ { name: "content-type", value: "application/json" } ]
      , body: Json.stringify $ Json.fromObject
          [ { name: "greeting", value: Json.fromString ("Hello, " <> name <> "!") }
          , { name: "query", value: Json.fromString request.query }
          , { name: "contentType"
            , value: fromMaybe Json.null (Json.fromString <$> header "content-type" request.headers)
            }
          ]
      }
  _ -> Async.fail MissingName

handler ::
  forall services. Has Probe services => ServerRequest -> Async RouteError services ServerResponse
handler request = do
  Probe probe <- Async.service @Probe
  case request.method, request.path of
    "POST", "/greet" -> greeting request
    "GET", "/crash" -> do
      Async.liftEffect (defect "handler crashed")
      pure (text 200 "unreachable")
    "GET", "/slow" -> Async.finally (Async.liftEffect (Signal.fire probe.released)) do
      Async.liftEffect (Signal.fire probe.started)
      Async.async \_ -> pure (pure unit)
    _, path -> Async.fail (UnknownRoute path)

describe ::
  forall services. Has (Http & Console) services => Request -> Async ProgramError services Unit
describe request = do
  response <- Async.mapError ClientFailed (Http.request request)
  Async.sync $ Console.log $ joinWith " "
    [ request.method
    , request.url
    , "->"
    , show response.status
    , fromMaybe "-" (header "content-type" response.headers)
    , response.body
    ]

program ::
  forall services.
  Has (HttpServer & Http & Console & Probe) services => Async ProgramError services Unit
program = do
  Probe probe <- Async.service @Probe
  server <- Async.mapError ServerFailed do
    HttpServer.serve { host: "127.0.0.1", port: 0, onError } handler
  let
    listening = HttpServer.address server
    origin = "http://" <> listening.host <> ":" <> show listening.port
    get path = { method: "GET", url: origin <> path, headers: [], body: Nothing }
  describe
    { method: "POST"
    , url: origin <> "/greet?from=test"
    , headers: [ { name: "content-type", value: "application/json" } ]
    , body: Just """{"name":"Alice"}"""
    }
  describe { method: "POST", url: origin <> "/greet", headers: [], body: Just "{}" }
  describe (get "/missing")
  describe (get "/crash")
  describe (get "/missing")
  pending <- Async.fork (Http.get (origin <> "/slow"))
  Async.sync (Console.log "slow request sent")
  Signal.wait probe.started
  Async.mapError ClientFailed (Async.kill (NetworkError "cancelled") pending)
  Async.sync (Console.log "slow request cancelled")
  Signal.wait probe.released
  Async.sync (Console.log "slow handler released")
  second <- Async.attempt do
    HttpServer.serve { host: "127.0.0.1", port: listening.port, onError } handler
  Async.sync $ Console.log case second of
    Left error -> "second listen rejected " <> show error
    Right _ -> "second listen accepted"
  HttpServer.close server
  Async.sync (Console.log "server closed")

run :: Effect Unit
run = do
  probe <- { started: _, released: _ } <$> Signal.new <*> Signal.new
  let
    environment =
      { server: HttpServer.default
      , http: HttpNode.client
      , console: Console.default
      , probe: Probe probe
      }
  void do
    Async.launch { onUncaught: \error -> Sync.run environment (Console.error (show error)) }
      environment
      program

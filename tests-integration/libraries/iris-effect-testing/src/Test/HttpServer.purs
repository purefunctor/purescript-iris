module Test.HttpServer
  ( HttpServer(..)
  , Address
  , ServerRequest
  , ServerResponse
  , Incoming
  , Listening
  , ServerError(..)
  , Server
  , serve
  , address
  , close
  , default
  ) where

import Prelude

import Data.Array as Array
import Data.Either (Either(..), either)
import Data.Maybe (Maybe(..))
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Defaults (class DefaultService)
import Iris.Effect.Fiber (Exit(..))
import Iris.Effect.Fiber as Fiber
import Iris.Effect.Http (Header)

type Address = { host :: String, port :: Int }

type ServerRequest =
  { method :: String
  , path :: String
  , query :: String
  , headers :: Array Header
  , body :: String
  }

type ServerResponse =
  { status :: Int
  , headers :: Array Header
  , body :: String
  }

type Incoming =
  { request :: ServerRequest
  , respond :: ServerResponse -> Effect Unit
  , onDisconnect :: Effect Unit -> Effect Unit
  }

type Listening =
  { address :: Address
  , close :: Effect Unit -> Effect Unit
  }

newtype HttpServer = HttpServer
  { listen ::
      Address ->
      (Incoming -> Effect Unit) ->
      (Either String Listening -> Effect Unit) ->
      Effect (Effect Unit)
  }

data ServerError = ListenError String

instance showServerError :: Show ServerError where
  show (ListenError message) = "(ListenError " <> show message <> ")"

data HandlerError error
  = ClientDisconnected
  | HandlerFailed error

newtype Server = Server { listening :: Listening, queue :: Queue }

type Queue =
  { pending :: Ref (Array (Maybe Incoming))
  , waiter :: Ref (Maybe (Maybe Incoming -> Effect Unit))
  }

serve ::
  forall error services.
  Has HttpServer services =>
  { host :: String, port :: Int, onError :: error -> ServerResponse } ->
  (ServerRequest -> Async error services ServerResponse) ->
  Async ServerError services Server
serve options handler = do
  HttpServer server <- Async.service @HttpServer
  queue <- Async.liftEffect newQueue
  listening <- Async.async \callback -> do
    cancel <- server.listen
      { host: options.host, port: options.port }
      (push queue <<< Just)
      (callback <<< either (Left <<< ListenError) Right)
    pure (Async.liftEffect cancel)
  _ <- Async.fork (accept queue options.onError handler)
  pure (Server { listening, queue })

address :: Server -> Address
address (Server server) = server.listening.address

close :: forall error services. Server -> Async error services Unit
close (Server server) = do
  Async.async \callback -> do
    server.listening.close (callback (Right unit))
    pure (pure unit)
  Async.liftEffect (push server.queue Nothing)

accept ::
  forall error other services.
  Queue ->
  (error -> ServerResponse) ->
  (ServerRequest -> Async error services ServerResponse) ->
  Async other services Unit
accept queue onError handler = do
  next <- dequeue queue
  case next of
    Nothing -> pure unit
    Just incoming -> do
      _ <- Async.fork (handle onError handler incoming)
      accept queue onError handler

handle ::
  forall error other services.
  (error -> ServerResponse) ->
  (ServerRequest -> Async error services ServerResponse) ->
  Incoming ->
  Async other services Unit
handle onError handler incoming = do
  fiber <- Async.fork (Async.mapError HandlerFailed (handler incoming.request))
  Async.liftEffect do
    incoming.onDisconnect (void (Fiber.killFiber ClientDisconnected fiber (pure unit)))
  exit <- Async.await fiber
  Async.liftEffect case exit of
    Succeeded response -> incoming.respond response
    Failed (HandlerFailed error) -> incoming.respond (onError error)
    Failed ClientDisconnected -> pure unit
    Died _ -> incoming.respond internalServerError

internalServerError :: ServerResponse
internalServerError =
  { status: 500
  , headers: [ { name: "content-type", value: "text/plain" } ]
  , body: "Internal Server Error"
  }

newQueue :: Effect Queue
newQueue = do
  pending <- Ref.new []
  waiter <- Ref.new Nothing
  pure { pending, waiter }

push :: Queue -> Maybe Incoming -> Effect Unit
push queue item = do
  waiter <- Ref.read queue.waiter
  case waiter of
    Just resume -> do
      Ref.write Nothing queue.waiter
      resume item
    Nothing -> Ref.modify_ (flip Array.snoc item) queue.pending

dequeue :: forall error services. Queue -> Async error services (Maybe Incoming)
dequeue queue = Async.async \callback -> do
  pending <- Ref.read queue.pending
  case Array.uncons pending of
    Just { head, tail } -> do
      Ref.write tail queue.pending
      callback (Right head)
      pure (pure unit)
    Nothing -> do
      Ref.write (Just (callback <<< Right)) queue.waiter
      pure (Async.liftEffect (Ref.write Nothing queue.waiter))

default :: HttpServer
default = HttpServer
  { listen: \listenAddress onIncoming callback ->
      listenNode listenAddress onIncoming (callback <<< Left) (callback <<< Right)
  }

instance defaultServiceHttpServer :: DefaultService HttpServer where
  defaultService = default

foreign import listenNode ::
  Address ->
  (Incoming -> Effect Unit) ->
  (String -> Effect Unit) ->
  (Listening -> Effect Unit) ->
  Effect (Effect Unit)

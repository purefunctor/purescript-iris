module Main (start, runScenario) where

import Prelude

import Catalog as Catalog
import Client as Client
import Effect (Effect)
import Iris.Effect.Async as Async
import Iris.Effect.Http.Node as HttpNode
import Logger (Logger(..))
import OrderStore as OrderStore
import ProductCache as ProductCache
import Routes as Routes
import Test.HttpServer as HttpServer

type Options =
  { host :: String
  , port :: Int
  , upstream :: String
  , cacheCapacity :: Int
  , log :: String -> Effect Unit
  , onListening :: { port :: Int, stop :: Effect Unit -> Effect Unit } -> Effect Unit
  , onFailure :: String -> Effect Unit
  }

start :: Options -> Effect Unit
start options = do
  cache <- ProductCache.make { capacity: options.cacheCapacity }
  orders <- OrderStore.make
  let
    environment =
      { server: HttpServer.default
      , catalog: Catalog.make { baseUrl: options.upstream } HttpNode.client
      , cache
      , orders
      , logger: Logger { write: options.log }
      }
    stop server done = void do
      Async.launch { onUncaught: options.onFailure } environment do
        HttpServer.close server
        Async.liftEffect done
  void do
    Async.launch { onUncaught: options.onFailure <<< show } environment do
      server <-
        HttpServer.serve { host: options.host, port: options.port, onError: Routes.errorResponse }
          Routes.handle
      Async.liftEffect
        (options.onListening { port: (HttpServer.address server).port, stop: stop server })

runScenario ::
  { baseUrl :: String
  , onResult :: Array String -> Effect Unit
  , onFailure :: String -> Effect Unit
  } -> Effect Unit
runScenario { baseUrl, onResult, onFailure } = void do
  Async.launch { onUncaught: onFailure <<< show } { http: HttpNode.client } do
    lines <- Client.scenario baseUrl
    Async.liftEffect (onResult lines)

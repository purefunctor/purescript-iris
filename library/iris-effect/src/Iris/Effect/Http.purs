module Iris.Effect.Http
  ( Http(..)
  , Header
  , Request
  , Response
  , HttpError(..)
  , request
  , get
  , default
  , ForeignRequest
  , ForeignClient
  , fromForeignClient
  ) where

import Prelude

import Data.Either (Either(..))
import Data.Maybe (Maybe(..), fromMaybe, isJust)
import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Defaults (class DefaultService)

type Header = { name :: String, value :: String }

type Request =
  { method :: String
  , url :: String
  , headers :: Array Header
  , body :: Maybe String
  }

type Response =
  { status :: Int
  , headers :: Array Header
  , body :: String
  }

data HttpError = NetworkError String

instance showHttpError :: Show HttpError where
  show (NetworkError message) = "(NetworkError " <> show message <> ")"

newtype Http = Http
  { request :: Request -> (Either HttpError Response -> Effect Unit) -> Effect (Effect Unit)
  }

request :: forall services. Has Http services => Request -> Async HttpError services Response
request parameters = do
  Http http <- Async.service @Http
  Async.async \callback -> do
    cancel <- http.request parameters callback
    pure (Async.liftEffect cancel)

get :: forall services. Has Http services => String -> Async HttpError services Response
get url = request { method: "GET", url, headers: [], body: Nothing }

default :: Http
default = fromForeignClient fetchClient

instance defaultServiceHttp :: DefaultService Http where
  defaultService = default

type ForeignRequest =
  { method :: String
  , url :: String
  , headers :: Array Header
  , body :: String
  , hasBody :: Boolean
  }

type ForeignClient =
  ForeignRequest -> (String -> Effect Unit) -> (Response -> Effect Unit) -> Effect (Effect Unit)

fromForeignClient :: ForeignClient -> Http
fromForeignClient client = Http
  { request: \parameters callback ->
      client
        { method: parameters.method
        , url: parameters.url
        , headers: parameters.headers
        , body: fromMaybe "" parameters.body
        , hasBody: isJust parameters.body
        }
        (callback <<< Left <<< NetworkError)
        (callback <<< Right)
  }

foreign import fetchClient :: ForeignClient

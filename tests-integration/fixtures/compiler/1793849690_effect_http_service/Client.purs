module Client (scenario) where

import Prelude

import Data.Maybe (Maybe(..))
import Data.Traversable (traverse)
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Http (Http, HttpError)
import Iris.Effect.Http as Http

type Call = { method :: String, path :: String, body :: Maybe String }

describe :: forall services. Has Http services => String -> Call -> Async HttpError services String
describe baseUrl { method, path, body } = do
  response <- Http.request
    { method
    , url: baseUrl <> path
    , headers: case body of
        Just _ -> [ { name: "content-type", value: "application/json" } ]
        Nothing -> []
    , body
    }
  pure (method <> " " <> path <> " " <> show response.status <> " " <> response.body)

scenario :: forall services. Has Http services => String -> Async HttpError services (Array String)
scenario baseUrl = traverse (describe baseUrl)
  [ { method: "GET", path: "/health", body: Nothing }
  , { method: "GET", path: "/products/p1", body: Nothing }
  , { method: "POST", path: "/orders", body: Just """{"product":"p1","quantity":2}""" }
  , { method: "GET", path: "/orders/1", body: Nothing }
  , { method: "POST", path: "/orders", body: Just """{"product":"p1","quantity":5000}""" }
  , { method: "POST", path: "/orders", body: Just "not json" }
  , { method: "POST", path: "/orders", body: Just """{"product":"p1","quantity":0}""" }
  , { method: "GET", path: "/products/missing", body: Nothing }
  , { method: "GET", path: "/products/broken", body: Nothing }
  , { method: "GET", path: "/orders/99", body: Nothing }
  , { method: "GET", path: "/orders/first", body: Nothing }
  , { method: "DELETE", path: "/orders", body: Nothing }
  , { method: "GET", path: "/crash", body: Nothing }
  , { method: "GET", path: "/health", body: Nothing }
  ]

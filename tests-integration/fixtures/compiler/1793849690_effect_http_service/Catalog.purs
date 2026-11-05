module Catalog (Catalog, CatalogError(..), Listing, make, listing, stock) where

import Prelude

import Data.Either (Either(..), note)
import Iris.Effect (class Has)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Http (Http)
import Iris.Effect.Http as Http
import Iris.Effect.Json (Json)
import Iris.Effect.Json as Json

data CatalogError = Missing | Unavailable String | Abandoned

type Listing = { name :: String, price :: Int }

newtype Catalog = Catalog { baseUrl :: String, http :: Http }

make :: { baseUrl :: String } -> Http -> Catalog
make { baseUrl } http = Catalog { baseUrl, http }

listing :: forall services. Has Catalog services => String -> Async CatalogError services Listing
listing identifier = do
  json <- fetchJson ("/catalog/" <> identifier)
  decoded <- decode do
    name <- note "missing name" (Json.field "name" json >>= Json.toString)
    price <- note "missing price" (Json.field "price" json >>= Json.toInt)
    pure { name, price }
  pure decoded

stock :: forall services. Has Catalog services => String -> Async CatalogError services Int
stock identifier = do
  json <- fetchJson ("/stock/" <> identifier)
  decode (note "missing available" (Json.field "available" json >>= Json.toInt))

decode :: forall services value. Either String value -> Async CatalogError services value
decode = case _ of
  Right value -> pure value
  Left message -> Async.fail (Unavailable ("invalid upstream payload: " <> message))

fetchJson :: forall services. Has Catalog services => String -> Async CatalogError services Json
fetchJson path = do
  Catalog catalog <- Async.service @Catalog
  response <- Async.provide { http: catalog.http }
    (Async.mapError (Unavailable <<< show) (Http.get (catalog.baseUrl <> path)))
  case response.status of
    200 -> decode (Json.parse response.body)
    404 -> Async.fail Missing
    status -> Async.fail (Unavailable ("upstream status " <> show status))

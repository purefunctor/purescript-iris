module Handlers (Shop, getProduct, createOrder, getOrder, encodeProduct, encodeOrder) where

import Prelude

import AppError (AppError(..))
import Catalog (Catalog, CatalogError(..))
import Catalog as Catalog
import Data.Either (Either(..), note)
import Data.Int as Int
import Data.Maybe (Maybe(..))
import Iris.Effect (class Has, type (&))
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Json (Json)
import Iris.Effect.Json as Json
import OrderStore (Order, OrderStore)
import OrderStore as OrderStore
import ProductCache (Product, ProductCache)
import ProductCache as ProductCache

type Shop = Catalog & ProductCache & OrderStore

fromCatalog :: String -> CatalogError -> AppError
fromCatalog identifier = case _ of
  Missing -> NotFound ("product " <> identifier)
  Unavailable reason -> UpstreamFailed reason
  Abandoned -> UpstreamFailed "abandoned"

getProduct ::
  forall services.
  Has (Catalog & ProductCache) services => String -> Async AppError services Product
getProduct identifier = do
  cached <- Async.sync (ProductCache.lookup identifier)
  case cached of
    Just product -> pure product
    Nothing -> do
      { listing, available } <- Async.mapError (fromCatalog identifier) do
        let
          fetchListing = Async.fork (Catalog.listing identifier)
        Async.bracket fetchListing (Async.kill Abandoned) \pending -> do
          available <- Catalog.stock identifier
          listing <- Async.join pending
          pure { listing, available }
      let
        product = { identifier, name: listing.name, price: listing.price, stock: available }
      Async.sync (ProductCache.insert product)
      pure product

createOrder ::
  forall services.
  Has Shop services =>
  String ->
  Async AppError services Order
createOrder body = do
  request <- either BadRequest (parseOrder body)
  product <- getProduct request.product
  when (product.stock < request.quantity) do
    Async.fail
      ( OutOfStock
          { product: product.identifier, requested: request.quantity, available: product.stock }
      )
  Async.sync
    ( OrderStore.create
        { product: product.identifier
        , quantity: request.quantity
        , total: product.price * request.quantity
        }
    )

parseOrder :: String -> Either String { product :: String, quantity :: Int }
parseOrder body = do
  json <- Json.parse body
  product <- note "missing field product" (Json.field "product" json >>= Json.toString)
  quantity <- note "missing field quantity" (Json.field "quantity" json >>= Json.toInt)
  if quantity > 0 then Right { product, quantity } else Left "quantity must be positive"

getOrder :: forall services. Has OrderStore services => String -> Async AppError services Order
getOrder text = case Int.fromString text of
  Nothing -> Async.fail (BadRequest ("invalid order number " <> text))
  Just number -> do
    found <- Async.sync (OrderStore.find number)
    case found of
      Just order -> pure order
      Nothing -> Async.fail (NotFound ("order " <> text))

either ::
  forall error services value.
  (String -> error) -> Either String value -> Async error services value
either wrap = case _ of
  Right value -> pure value
  Left message -> Async.fail (wrap message)

encodeProduct :: Product -> Json
encodeProduct product = Json.fromObject
  [ { name: "id", value: Json.fromString product.identifier }
  , { name: "name", value: Json.fromString product.name }
  , { name: "price", value: Json.fromInt product.price }
  , { name: "stock", value: Json.fromInt product.stock }
  ]

encodeOrder :: Order -> Json
encodeOrder order = Json.fromObject
  [ { name: "number", value: Json.fromInt order.number }
  , { name: "product", value: Json.fromString order.product }
  , { name: "quantity", value: Json.fromInt order.quantity }
  , { name: "total", value: Json.fromInt order.total }
  ]

module ProductCache (ProductCache, Product, make, lookup, insert) where

import Prelude

import Data.Map (Map)
import Data.Map as Map
import Data.Maybe (Maybe(..))
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Iris.Effect (class Has)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

type Product = { identifier :: String, name :: String, price :: Int, stock :: Int }

type Entries = { products :: Map String Product, arrivals :: Map Int String, next :: Int }

newtype ProductCache = ProductCache { capacity :: Int, entries :: Ref Entries }

make :: { capacity :: Int } -> Effect ProductCache
make { capacity } = do
  entries <- Ref.new { products: Map.empty, arrivals: Map.empty, next: 0 }
  pure (ProductCache { capacity, entries })

lookup :: forall services. Has ProductCache services => String -> Sync services (Maybe Product)
lookup identifier = do
  ProductCache cache <- Sync.service @ProductCache
  entries <- Sync.liftEffect (Ref.read cache.entries)
  pure (Map.lookup identifier entries.products)

insert :: forall services. Has ProductCache services => Product -> Sync services Unit
insert product = do
  ProductCache cache <- Sync.service @ProductCache
  Sync.liftEffect (Ref.modify_ (store cache.capacity product) cache.entries)

store :: Int -> Product -> Entries -> Entries
store capacity product entries
  | Map.member product.identifier entries.products =
    entries { products = Map.insert product.identifier product entries.products }
  | otherwise =
    let
      admitted = evict capacity entries
    in
      { products: Map.insert product.identifier product admitted.products
      , arrivals: Map.insert admitted.next product.identifier admitted.arrivals
      , next: admitted.next + 1
      }

evict :: Int -> Entries -> Entries
evict capacity entries
  | Map.size entries.products < capacity = entries
  | otherwise = case Map.findMin entries.arrivals of
    Just oldest ->
      entries
        { products = Map.delete oldest.value entries.products
        , arrivals = Map.delete oldest.key entries.arrivals
        }
    Nothing -> entries

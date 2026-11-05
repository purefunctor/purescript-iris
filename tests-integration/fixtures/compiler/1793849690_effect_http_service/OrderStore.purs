module OrderStore (OrderStore, Order, make, create, find) where

import Prelude

import Data.Map (Map)
import Data.Map as Map
import Data.Maybe (Maybe)
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Iris.Effect (class Has)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

type Order = { number :: Int, product :: String, quantity :: Int, total :: Int }

newtype OrderStore = OrderStore (Ref { orders :: Map Int Order, next :: Int })

make :: Effect OrderStore
make = OrderStore <$> Ref.new { orders: Map.empty, next: 1 }

create ::
  forall services.
  Has OrderStore services =>
  { product :: String, quantity :: Int, total :: Int } ->
  Sync services Order
create { product, quantity, total } = do
  OrderStore store <- Sync.service @OrderStore
  Sync.liftEffect do
    state <- Ref.read store
    let
      order = { number: state.next, product, quantity, total }
    Ref.write { orders: Map.insert order.number order state.orders, next: state.next + 1 } store
    pure order

find :: forall services. Has OrderStore services => Int -> Sync services (Maybe Order)
find number = do
  OrderStore store <- Sync.service @OrderStore
  state <- Sync.liftEffect (Ref.read store)
  pure (Map.lookup number state.orders)

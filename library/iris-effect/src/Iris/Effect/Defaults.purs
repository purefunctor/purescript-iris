module Iris.Effect.Defaults
  ( class DefaultService
  , defaultService
  , class DefaultServices
  , defaultServices
  , defaults
  , runDefaults
  , launchDefaults
  ) where

import Prelude

import Data.Symbol (class IsSymbol)
import Effect (Effect)
import Iris.Effect.Async (Async, Fiber)
import Iris.Effect.Async as Async
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync
import Prim.Row as Row
import Prim.RowList (RowList)
import Prim.RowList as RowList
import Record.Builder (Builder)
import Record.Builder as Builder
import Type.Proxy (Proxy(..))

class DefaultService service where
  defaultService :: service

class DefaultServices :: RowList Type -> Row Type -> Constraint
class DefaultServices list services | list -> services where
  defaultServices :: Proxy list -> Builder (Record ()) (Record services)

instance defaultServicesNil :: DefaultServices RowList.Nil () where
  defaultServices _ = identity

instance defaultServicesCons ::
  ( IsSymbol label
  , DefaultService service
  , DefaultServices tail services
  , Row.Lacks label services
  , Row.Cons label service services extended
  ) =>
  DefaultServices (RowList.Cons label service tail) extended where
  defaultServices _ =
    Builder.insert (Proxy :: Proxy label) (defaultService :: service)
      <<< defaultServices (Proxy :: Proxy tail)

defaults ::
  forall @services list.
  RowList.RowToList services list => DefaultServices list services => Record services
defaults = Builder.buildFromScratch (defaultServices (Proxy :: Proxy list))

runDefaults ::
  forall @services list value.
  RowList.RowToList services list =>
  DefaultServices list services =>
  Sync services value ->
  Effect value
runDefaults = Sync.run (defaults @services)

launchDefaults ::
  forall @services list error value.
  RowList.RowToList services list =>
  DefaultServices list services =>
  { onUncaught :: error -> Effect Unit } ->
  Async error services value ->
  Effect (Fiber error value)
launchDefaults options = Async.launch options (defaults @services)

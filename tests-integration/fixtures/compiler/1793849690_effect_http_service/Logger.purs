module Logger (Logger(..), info) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

newtype Logger = Logger { write :: String -> Effect Unit }

info :: forall services. Has Logger services => String -> Sync services Unit
info message = do
  Logger logger <- Sync.service @Logger
  Sync.liftEffect (logger.write message)

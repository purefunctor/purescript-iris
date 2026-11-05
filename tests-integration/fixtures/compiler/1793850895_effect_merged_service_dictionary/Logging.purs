module Logging (Logger(..), logLine) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

newtype Logger = Logger (String -> Effect Unit)

logLine :: forall services. Has Logger services => String -> Sync services Unit
logLine message = do
  Logger write <- Sync.service @Logger
  Sync.liftEffect (write message)

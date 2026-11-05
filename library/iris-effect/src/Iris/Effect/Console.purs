module Iris.Effect.Console
  ( Console(..)
  , log
  , error
  , default
  ) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Defaults (class DefaultService)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

newtype Console = Console
  { log :: String -> Effect Unit
  , error :: String -> Effect Unit
  }

log :: forall services. Has Console services => String -> Sync services Unit
log message = do
  Console console <- Sync.service @Console
  Sync.liftEffect (console.log message)

error :: forall services. Has Console services => String -> Sync services Unit
error message = do
  Console console <- Sync.service @Console
  Sync.liftEffect (console.error message)

default :: Console
default = Console { log: logToConsole, error: errorToConsole }

foreign import logToConsole :: String -> Effect Unit

foreign import errorToConsole :: String -> Effect Unit

instance defaultServiceConsole :: DefaultService Console where
  defaultService = default

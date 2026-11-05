module Main where

import Iris.Effect (class Has, getService)
import Services (Console(..), Http(..))

lookup :: forall @service services. Has service services => Record services -> service
lookup = getService

inferred environment = case lookup @Console environment of
  Console value -> value

multiple environment =
  { console: lookup @Console environment
  , http: lookup @Http environment
  }

delayed :: forall tail. Has Console (network :: Http | tail) => Record (network :: Http | tail) -> Console
delayed = lookup @Console

inferredValue :: Int
inferredValue = inferred { logger: Console 37 }

multipleValue :: { console :: Console, http :: Http }
multipleValue = multiple { logger: Console 19, network: Http 23 }

delayedValue :: Console
delayedValue = delayed { network: Http 4, logger: Console 61 }

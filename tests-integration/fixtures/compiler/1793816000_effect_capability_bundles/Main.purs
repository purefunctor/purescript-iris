module Main where

import Prelude

import Capabilities (First(..), Second(..), Shop, Tagged(..))
import Iris.Effect (class Has, getService, type (&))

reading :: forall services. Has (Shop & First) services => Record services -> Int
reading services = case getService services, getService services of
  First first, Second second -> first * 100 + second

inferred services = reading services

ranked
  :: forall services
   . Has Shop services
  => (forall other. Has (First & Second) other => Record other -> Int)
  -> Record services
  -> Int
ranked action = action

tagged :: forall value services. Has (First & Tagged value & Second) services => Record services -> value
tagged services = case getService services of
  Tagged value -> value

readingValue :: Int
readingValue = reading { first: First 3, second: Second 7 }

inferredValue :: Int
inferredValue = inferred { red: First 11, blue: Second 13 }

rankedValue :: Int
rankedValue = ranked reading { second: Second 2, first: First 5 }

taggedValue :: String
taggedValue = tagged { first: First 0, second: Second 0, tagged: Tagged "tagged" }

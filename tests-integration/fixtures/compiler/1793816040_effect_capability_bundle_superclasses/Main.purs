module Main where

import Prelude

import Iris.Effect (class Has, getService, type (&))

newtype First = First Int

newtype Second = Second Int

class Has (First & Second) services <= Both services

instance Has (First & Second) services => Both services

reading :: forall services. Both services => Record services -> Int
reading services = case getService services, getService services of
  First first, Second second -> first * 100 + second

readingValue :: Int
readingValue = reading { first: First 3, second: Second 7 }

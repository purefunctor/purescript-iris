module Capabilities where

import Prelude

import Iris.Effect (class Has, getService, type (&))

newtype First = First Int

newtype Second = Second Int

type Shop = First & Second

reading :: forall services. Has Shop services => Record services -> Int
reading services = case getService services, getService services of
  First first, Second second -> first * 100 + second

inferred services = reading services

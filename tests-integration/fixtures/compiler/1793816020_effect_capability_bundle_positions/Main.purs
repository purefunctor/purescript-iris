module Main where

import Iris.Effect (class Has, getService, type (&))

data First

data Second

type Shop = First & Second

type Ignore :: Type -> Type
type Ignore value = Int

invalidValue :: Shop -> Shop
invalidValue value = value

invalidService :: Record (first :: First, second :: Second) -> Shop
invalidService = getService

data InvalidField = InvalidField (Array Shop)

invalidErased :: Ignore (First & Second)
invalidErased = 0

lookup :: forall @service services. Has service services => Record services -> service
lookup = getService

invalidApplication :: Record (first :: First, second :: Second) -> Int
invalidApplication services = case lookup @Shop services of
  _ -> 0

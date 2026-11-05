module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

whenConfigured :: { enabled :: Boolean, value :: Int } -> Ref.Ref Int -> Sync () Unit
whenConfigured configuration reference =
  when configuration.enabled (Sync.liftEffect (Ref.write configuration.value reference))

unlessConfigured :: { enabled :: Boolean, value :: Int } -> Ref.Ref Int -> Sync () Unit
unlessConfigured configuration reference =
  unless configuration.enabled (Sync.liftEffect (Ref.write configuration.value reference))

step :: Ref.Ref Int -> Int -> Sync () Int
step reference limit = do
  current <- Sync.liftEffect (Ref.read reference)
  when (current < limit) (Sync.liftEffect (Ref.write (current + 1) reference))
  unless (current < limit) (Sync.liftEffect (Ref.write 0 reference))
  Sync.liftEffect (Ref.read reference)

run :: forall value. Sync () value -> Effect value
run = Sync.run {}

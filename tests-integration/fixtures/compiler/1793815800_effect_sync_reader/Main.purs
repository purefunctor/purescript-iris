module Main (run) where

import Prelude

import Effect (Effect)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

type Report = String -> Effect Unit

greeting :: forall services. Sync (name :: String | services) String
greeting = Sync.make \{ name } -> pure ("hello " <> name)

say :: forall services. Report -> String -> Sync services Unit
say report message = Sync.liftEffect (report message)

program :: Report -> Sync (name :: String, count :: Int) Unit
program report = do
  say report "start"
  outer <- greeting
  inner <- Sync.provide { name: "inner" } greeting
  count <- Sync.make \{ count } -> pure count
  combined <- (\left right -> left <> " & " <> right) <$> greeting <*> map show (pure count)
  say report (outer <> ", " <> inner <> ", " <> combined)

run :: Report -> Effect Unit
run report = do
  let
    action = Sync.run { name: "outer", count: 2 } (program report)
  report "built"
  action
  action

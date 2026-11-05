module Main (run) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has, type (&))
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

type Report = String -> Effect Unit

newtype Name = Name String

newtype Count = Count Int

greeting :: forall services. Has Name services => Sync services String
greeting = do
  Name name <- Sync.service @Name
  pure ("hello " <> name)

say :: forall services. Report -> String -> Sync services Unit
say report message = Sync.liftEffect (report message)

program :: forall services. Has (Name & Count) services => Report -> Sync services Unit
program report = do
  say report "start"
  outer <- greeting
  inner <- Sync.provide { renamed: Name "inner" } greeting
  Count count <- Sync.service @Count
  combined <- (\left right -> left <> " & " <> right) <$> greeting <*> map show (pure count)
  say report (outer <> ", " <> inner <> ", " <> combined)

run :: Report -> Effect Unit
run report = do
  let
    action = Sync.run { name: Name "outer", count: Count 2 } (program report)
  report "built"
  action
  action

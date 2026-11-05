module Main (run) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has)
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync
import Logging (Logger(..), logLine)

newtype Clock = Clock Int

newtype Label value = Label value

handle :: forall services. Has Logger services => Has Clock services => String -> Sync services Unit
handle request = do
  Clock now <- Sync.service @Clock
  logLine (show now <> " " <> request)

labelled
  :: forall @value services
   . Show value
  => Has (Label value) services
  => Has Logger services
  => Sync services Unit
labelled = do
  Label value <- Sync.service @(Label value)
  logLine (show value)

run :: (String -> Effect Unit) -> Effect Unit
run write = Sync.run { clock: Clock 12, log: Logger write, label: Label true } do
  handle "first"
  labelled @Boolean

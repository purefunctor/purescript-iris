module Main (greet, fetchGreeting) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has, type (&))
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Console (Console)
import Iris.Effect.Console as Console
import Iris.Effect.Defaults (launchDefaults, runDefaults)
import Iris.Effect.Http (Http, HttpError)
import Iris.Effect.Http as Http
import Iris.Effect.Sync (Sync)

welcome :: forall services. Has Console services => String -> Sync services Unit
welcome name = do
  Console.log ("Welcome, " <> name)
  Console.error ("Audit: " <> name)

greet :: Effect Unit
greet = runDefaults @(console :: Console) do
  welcome "Alice"
  welcome "Bob"

fetched :: forall services. Has (Console & Http) services => String -> Async HttpError services Unit
fetched url = do
  response <- Http.get url
  Async.sync (Console.log (show response.status <> " " <> response.body))

fetchGreeting :: String -> Effect Unit
fetchGreeting url = void do
  launchDefaults @(console :: Console, http :: Http)
    { onUncaught: \error -> runDefaults @(console :: Console) (Console.error (show error)) }
    (fetched url)

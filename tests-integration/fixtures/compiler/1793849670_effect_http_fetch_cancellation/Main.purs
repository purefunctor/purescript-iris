module Main (cancelFetch) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Defaults (launchDefaults)
import Iris.Effect.Fiber (Exit(..))
import Iris.Effect.Http (Http, HttpError(..))
import Iris.Effect.Http as Http

type Options =
  { url :: String
  , onArrived :: Effect Unit -> Effect Unit
  , report :: String -> Effect Unit
  }

arrival :: forall error services. Options -> Async error services Unit
arrival options = Async.async \callback -> do
  options.onArrived (callback (Right unit))
  pure (pure unit)

cancelFetch :: Options -> Effect Unit
cancelFetch options = void do
  launchDefaults @(http :: Http) { onUncaught: options.report <<< show } do
    pending <- Async.fork (Http.get options.url)
    arrival options
    Async.kill (NetworkError "cancelled") pending
    exit <- Async.await pending
    Async.liftEffect $ options.report case exit of
      Succeeded response -> "succeeded " <> show response.status
      Failed error -> "failed " <> show error
      Died _ -> "died"

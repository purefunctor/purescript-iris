module Main where

import Prelude

import Effect (Effect)
import Iris.Effect.Async (Async)
import Iris.Effect.Console (Console)
import Iris.Effect.Console as Console
import Iris.Effect.Http (Http, HttpError, Response)
import Iris.Effect.Http as Http
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

undeclared :: String -> Async HttpError (console :: Console) Response
undeclared url = Http.get url

unconstrained :: forall services. String -> Sync services Unit
unconstrained message = Console.log message

unprovided :: Effect Unit
unprovided = Sync.run { http: Http.default } (Console.log "No console")

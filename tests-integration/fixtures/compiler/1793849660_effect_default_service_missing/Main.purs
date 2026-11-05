module Main where

import Prelude

import Effect (Effect)
import Iris.Effect.Console (Console)
import Iris.Effect.Console as Console
import Iris.Effect.Defaults (runDefaults)
import Iris.Effect.Http (Http)

newtype Logger = Logger (String -> Effect Unit)

notInRow :: Effect Unit
notInRow = runDefaults @(http :: Http) (Console.log "No console")

noDefault :: Effect Unit
noDefault = runDefaults @(console :: Console, logger :: Logger) (Console.log "No default logger")

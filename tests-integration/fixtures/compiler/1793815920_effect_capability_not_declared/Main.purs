module Main where

import Iris.Effect (getService)

newtype Console = Console Int

newtype Http = Http Int

missing :: Record (console :: Console) -> Http
missing = getService

wrongIdentity :: Record (http :: Console) -> Http
wrongIdentity = getService

annotated = (getService :: Record () -> Console)

nested environment = local environment
  where
  local :: Record (console :: Console) -> Http
  local = getService

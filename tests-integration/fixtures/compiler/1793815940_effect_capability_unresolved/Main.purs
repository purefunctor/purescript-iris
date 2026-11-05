module Main where

import Iris.Effect (getService)

newtype Console = Console Int

undeclared :: forall services. Record services -> Console
undeclared = getService

open :: forall tail. Record (console :: Console | tail) -> Console
open = getService

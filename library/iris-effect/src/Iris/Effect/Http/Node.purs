module Iris.Effect.Http.Node
  ( client
  ) where

import Iris.Effect.Http (ForeignClient, Http, fromForeignClient)

client :: Http
client = fromForeignClient keepAliveClient

foreign import keepAliveClient :: ForeignClient

module Main where

import Prelude

import Effect (Effect)

foreign import record :: String -> Effect Unit

main :: Effect Unit
main = do
  record "start"
  name <- pure "Iris"
  record name

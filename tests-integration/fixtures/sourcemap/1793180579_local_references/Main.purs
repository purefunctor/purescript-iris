module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

doubled :: Int -> Effect Int
doubled value = do
  counter <- Ref.new value
  _ <- Ref.modify (_ * 2) counter
  Ref.read counter

module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

freshEffect :: Effect (Ref.Ref (Effect Int))
freshEffect = Ref.new (pure 1)

freshFunction :: Effect (Ref.Ref (Int -> Int))
freshFunction = Ref.new (\x -> x + 1)

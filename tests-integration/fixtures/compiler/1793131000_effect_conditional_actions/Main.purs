module Main where

import Prelude

import Effect (Effect, forE)
import Effect.Ref as Ref

whenConfigured :: { enabled :: Boolean, value :: Int } -> Ref.Ref Int -> Effect Unit
whenConfigured configuration reference =
  when configuration.enabled (Ref.write configuration.value reference)

unlessConfigured :: { enabled :: Boolean, value :: Int } -> Ref.Ref Int -> Effect Unit
unlessConfigured configuration reference =
  unless configuration.enabled (Ref.write configuration.value reference)

diagonal :: Int -> Effect Int
diagonal size = do
  count <- Ref.new 0
  forE 0 size \row -> forE 0 size \column -> when (row == column) (Ref.modify_ (_ + 1) count)
  Ref.read count

genericWhen :: forall effect. Applicative effect => Boolean -> effect Unit -> effect Unit
genericWhen = when

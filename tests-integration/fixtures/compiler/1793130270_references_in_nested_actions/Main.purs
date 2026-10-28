module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

captured :: Boolean -> Effect Int
captured flag = do
  ref <- Ref.new 0
  when flag (Ref.modify_ (_ + 5) ref)
  if flag then Ref.modify_ (_ * 2) ref else pure unit
  let increment = Ref.modify_ (_ + 1) ref
  increment
  increment
  Ref.read ref

capturedAction :: Effect (Effect Int)
capturedAction = do
  ref <- Ref.new 0
  let bump = Ref.modify (_ + 1) ref
  _ <- bump
  pure bump

counter :: Effect { increment :: Effect Unit, current :: Effect Int }
counter = do
  ref <- Ref.new 0
  let increment = Ref.modify_ (_ + 1) ref
  increment
  pure { increment, current: Ref.read ref }

module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

bump :: Ref Int -> Effect Unit
bump ref = Ref.modify_ (_ + 1) ref

passed :: Effect Int
passed = do
  ref <- Ref.new 0
  bump ref
  Ref.modify_ (_ + 10) ref
  Ref.read ref

returned :: Effect (Ref Int)
returned = do
  ref <- Ref.new 1
  Ref.write 2 ref
  pure ref

stored :: Effect { counter :: Ref Int }
stored = do
  ref <- Ref.new 3
  Ref.modify_ (_ * 2) ref
  pure { counter: ref }

captured :: Boolean -> Effect Int
captured flag = do
  ref <- Ref.new 0
  when flag (Ref.modify_ (_ + 5) ref)
  if flag then Ref.modify_ (_ * 2) ref else pure unit
  let increment = Ref.modify_ (_ + 1) ref
  increment
  increment
  Ref.read ref

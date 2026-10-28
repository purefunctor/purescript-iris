module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

performedInside :: Effect Int
performedInside = do
  ref <- Ref.new 1
  let
    doubled = unsafePerformEffect do
      Ref.modify_ (_ * 2) ref
      Ref.read ref
  Ref.modify_ (_ + doubled) ref
  Ref.read ref

runInside :: Int -> Int
runInside start = unsafePerformEffect do
  ref <- Ref.new start
  let
    inner = ST.run do
      local <- STRef.new start
      let bump = STRef.modify (_ + 1) local
      _ <- bump
      bump
  Ref.modify_ (_ + inner) ref
  Ref.read ref

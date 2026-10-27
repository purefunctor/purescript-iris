module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Control.Monad.ST.Uncurried (STFn2, mkSTFn2, runSTFn2)
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Effect.Uncurried (EffectFn2, mkEffectFn2, runEffectFn2)

foreign import addEffect :: EffectFn2 Int Int Int

addTo :: EffectFn2 (Ref Int) Int Int
addTo = mkEffectFn2 \ref amount -> Ref.modify (_ + amount) ref

called :: Effect { added :: Int, external :: Int }
called = do
  ref <- Ref.new 1
  added <- runEffectFn2 addTo ref 2
  external <- runEffectFn2 addEffect 3 4
  pure { added, external }

partiallyApplied :: Int -> Effect Int
partiallyApplied = runEffectFn2 addEffect 1

multiplyBy :: forall region. STFn2 (STRef.STRef region Int) Int region Int
multiplyBy = mkSTFn2 \ref factor -> STRef.modify (_ * factor) ref

multiplied :: Int
multiplied = ST.run do
  ref <- STRef.new 6
  runSTFn2 multiplyBy ref 7

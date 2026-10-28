module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Effect.Uncurried (EffectFn1, EffectFn2, mkEffectFn2, runEffectFn1, runEffectFn2)

addToReference :: EffectFn2 (Ref Int) Int Int
addToReference = mkEffectFn2 \reference amount -> Ref.modify (_ + amount) reference

addFive :: EffectFn2 (Ref Int) Int Int -> Ref Int -> Effect Int
addFive function reference = runEffectFn2 function reference 5

logTwice :: EffectFn1 String Unit -> String -> Effect Unit
logTwice log message = do
  runEffectFn1 log message
  runEffectFn1 log (message <> "!")

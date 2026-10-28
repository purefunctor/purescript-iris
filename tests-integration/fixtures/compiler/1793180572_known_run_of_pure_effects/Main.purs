module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect.Unsafe (unsafePerformEffect)

performPure :: Int -> Int
performPure value = unsafePerformEffect (pure (value + 1))

runPure :: Int -> Int
runPure value = ST.run (pure (value * 2))

runReference :: Int -> Int
runReference value = ST.run (STRef.new value >>= STRef.read)

module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref (STRef)
import Control.Monad.ST.Ref as STRef
import Control.Monad.ST.Uncurried (STFn2, mkSTFn2, runSTFn2)

addToReference :: forall r. STFn2 (STRef r Int) Int r Int
addToReference = mkSTFn2 \reference amount -> STRef.modify (_ + amount) reference

addTwice :: Int -> Int
addTwice value = ST.run do
  reference <- STRef.new value
  _ <- runSTFn2 addToReference reference 2
  runSTFn2 addToReference reference 3

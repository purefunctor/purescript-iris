module Main where

import Prelude

import Control.Monad.ST (ST)
import Control.Monad.ST as ST
import Control.Monad.ST.Ref (STRef)
import Control.Monad.ST.Ref as STRef

newCounter :: forall r. Int -> ST r (STRef r Int)
newCounter value = STRef.new value

readCounter :: forall r. STRef r Int -> ST r Int
readCounter counter = STRef.read counter

resetCounter :: forall r. STRef r Int -> ST r Int
resetCounter counter = STRef.write 0 counter

incrementCounter :: forall r. STRef r Int -> ST r Int
incrementCounter counter = STRef.modify (_ + 1) counter

countTwice :: Int -> Int
countTwice value = ST.run do
  counter <- STRef.new value
  _ <- STRef.modify (_ * 2) counter
  written <- STRef.write (value + 1) counter
  next <- STRef.modify (_ * 10) counter
  current <- STRef.read counter
  pure (written + next + current)

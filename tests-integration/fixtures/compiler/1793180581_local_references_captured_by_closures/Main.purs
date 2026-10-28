module Main where

import Prelude

import Control.Monad.ST (ST)
import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref

delayedRead :: Int -> Effect Int
delayedRead value = do
  reference <- Ref.new value
  let readLater = Ref.read reference
  Ref.write (value + 6) reference
  readLater

closureWrites :: Effect Int
closureWrites = do
  reference <- Ref.new 0
  let increment = Ref.modify (_ + 1) reference
  _ <- increment
  _ <- increment
  Ref.read reference

countDown :: forall r. Int -> ST r Int
countDown start = do
  remaining <- STRef.new start
  steps <- STRef.new 0
  ST.while ((_ > 0) <$> STRef.read remaining) do
    _ <- STRef.modify (_ - 1) remaining
    void $ STRef.modify (_ + 1) steps
  STRef.read steps

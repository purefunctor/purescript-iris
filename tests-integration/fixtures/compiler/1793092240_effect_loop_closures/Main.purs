module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, forE, foreachE)
import Effect.Ref as Ref

rangeClosures :: Effect (Array (Unit -> Int))
rangeClosures = do
  closures <- Ref.new []
  forE 0 3 \index -> Ref.modify_ (_ <> [ \_ -> index ]) closures
  Ref.read closures

elementClosures :: Array String -> Effect (Array (Unit -> String))
elementClosures values = do
  closures <- Ref.new []
  foreachE values \value -> Ref.modify_ (_ <> [ \_ -> value ]) closures
  Ref.read closures

stateClosures :: Array (Unit -> Int)
stateClosures = ST.run do
  closures <- STRef.new []
  ST.for 5 8 \index -> STRef.modify (_ <> [ \_ -> index ]) closures
  STRef.read closures

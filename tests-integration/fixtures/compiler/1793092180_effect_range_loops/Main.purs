module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, forE)
import Effect.Ref (Ref)
import Effect.Ref as Ref

foreign import traced :: String -> Int -> Int

sumRange :: Int -> Int -> Effect Int
sumRange start end = do
  total <- Ref.new 0
  forE (traced "start" start) (traced "end" end) \index -> Ref.modify_ (_ + index) total
  Ref.read total

addTo :: Ref Int -> Int -> Effect Unit
addTo total index = Ref.modify_ (_ + index) total

sumWithFunction :: Int -> Effect Int
sumWithFunction end = do
  total <- Ref.new 0
  forE 0 end (addTo total)
  Ref.read total

productRange :: Int -> Int
productRange end = ST.run do
  total <- STRef.new 1
  ST.for 1 end \index -> STRef.modify (_ * index) total
  STRef.read total

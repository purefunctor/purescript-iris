module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

readBeforeWrite :: Int -> Effect Int
readBeforeWrite value = do
  reference <- Ref.new value
  before <- Ref.read reference
  Ref.write (value + 4) reference
  after <- Ref.read reference
  pure (before * 10 + after)

sumTwo :: Int -> Int -> Effect Int
sumTwo left right = do
  first <- Ref.new left
  second <- Ref.new right
  _ <- Ref.modify (_ + 1) first
  a <- Ref.read first
  b <- Ref.read second
  pure (a + b)

escapes :: Int -> Effect (Ref Int)
escapes value = do
  reference <- Ref.new value
  Ref.write (value * 2) reference
  pure reference

passedToFunction :: Int -> Effect Int
passedToFunction value = do
  reference <- Ref.new value
  Ref.modify_ (_ + 1) reference
  Ref.read reference

module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

newCounter :: Int -> Effect (Ref Int)
newCounter value = Ref.new value

readCounter :: Ref Int -> Effect Int
readCounter counter = Ref.read counter

resetCounter :: Ref Int -> Effect Unit
resetCounter counter = Ref.write 0 counter

incrementCounter :: Ref Int -> Effect Int
incrementCounter counter = Ref.modify (_ + 1) counter

addToField :: Int -> { counter :: Ref Int } -> Effect Int
addToField step state = Ref.modify (add step) state.counter

countTwice :: Int -> Effect Int
countTwice value = do
  counter <- Ref.new value
  _ <- Ref.modify (_ * 2) counter
  Ref.write (value + 1) counter
  next <- Ref.modify (_ * 10) counter
  current <- Ref.read counter
  pure (next + current)
